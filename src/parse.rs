//! Converts the pulldown-cmark event stream into the block model.

use std::rc::Rc;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, MetadataBlockKind, Options,
    Parser, Tag, TagEnd,
};

use crate::html;
use crate::model::{
    AlertKind, Align, Atom, Block, Document, FrontMatterKind, InlineText, ListItem, Segment, Table,
};
use crate::options::LinkMode;
use crate::style::{Style, Theme};

fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
}

fn convert_alert(kind: BlockQuoteKind) -> AlertKind {
    match kind {
        BlockQuoteKind::Note => AlertKind::Note,
        BlockQuoteKind::Tip => AlertKind::Tip,
        BlockQuoteKind::Important => AlertKind::Important,
        BlockQuoteKind::Warning => AlertKind::Warning,
        BlockQuoteKind::Caution => AlertKind::Caution,
    }
}

/// What an open inline HTML tag did to the builder, undone when the tag closes.
enum HtmlEffect {
    Style,
    Target,
    Nothing,
}

/// Accumulates inline events into atoms. Words are closed by whitespace or breaks.
pub(crate) struct InlineBuilder<'t> {
    theme: &'t Theme,
    links: LinkMode,
    atoms: Vec<Atom>,
    word: Vec<Segment>,
    styles: Vec<Style>,
    link: Option<Rc<str>>,
    /// Open links or images: (url, is_image, text collected so far).
    targets: Vec<(Rc<str>, bool, String)>,
    /// Open inline HTML tags, innermost last.
    html_open: Vec<(String, HtmlEffect)>,
}

impl<'t> InlineBuilder<'t> {
    pub(crate) fn new(theme: &'t Theme, links: LinkMode, base: Style) -> InlineBuilder<'t> {
        InlineBuilder {
            theme,
            links,
            atoms: Vec::new(),
            word: Vec::new(),
            styles: vec![base],
            link: None,
            targets: Vec::new(),
            html_open: Vec::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.atoms.is_empty() && self.word.is_empty()
    }

    /// Applies one HTML tag: styles, `<br>`, `<a>`, `<img>`. Unknown tags are ignored.
    pub(crate) fn html_tag(&mut self, tag: &html::Tag) {
        if tag.closing {
            let Some(pos) = self.html_open.iter().rposition(|(n, _)| *n == tag.name) else {
                return;
            };
            while self.html_open.len() > pos {
                let (_, effect) = self.html_open.pop().expect("len > pos");
                match effect {
                    HtmlEffect::Style => self.pop_style(),
                    HtmlEffect::Target => self.end_target(),
                    HtmlEffect::Nothing => {}
                }
            }
            return;
        }
        match tag.name.as_str() {
            "br" => self.hard_break(),
            "img" => {
                let src = tag.attr("src").unwrap_or_default();
                let alt = tag.attr("alt").filter(|a| !a.trim().is_empty());
                self.start_target(&src, true);
                self.text(alt.as_deref().unwrap_or("image"));
                self.end_target();
            }
            "a" => {
                let href = tag.attr("href").unwrap_or_default();
                self.start_target(&href, false);
                self.html_open.push((tag.name.clone(), HtmlEffect::Target));
            }
            "wbr" | "hr" | "input" | "source" | "picture" | "meta" | "link" => {}
            name => {
                let effect = match html::inline_style(name, self.theme) {
                    Some(style) => {
                        self.push_style(style);
                        HtmlEffect::Style
                    }
                    None => HtmlEffect::Nothing,
                };
                if !tag.self_closing {
                    self.html_open.push((tag.name.clone(), effect));
                }
            }
        }
    }

    pub(crate) fn style(&self) -> Style {
        *self.styles.last().expect("base style")
    }

    pub(crate) fn push_style(&mut self, over: Style) {
        let s = self.style().merge(over);
        self.styles.push(s);
    }

    pub(crate) fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    pub(crate) fn end_word(&mut self) {
        if !self.word.is_empty() {
            self.atoms.push(Atom::Word(std::mem::take(&mut self.word)));
        }
    }

    pub(crate) fn space(&mut self) {
        if matches!(self.atoms.last(), Some(Atom::Word(_))) {
            self.atoms.push(Atom::Space);
        }
    }

    fn append(&mut self, text: &str, style: Style, link: Option<Rc<str>>) {
        if text.is_empty() {
            return;
        }
        if let Some((_, _, collected)) = self.targets.last_mut() {
            collected.push_str(text);
        }
        match self.word.last_mut() {
            Some(last) if last.style == style && last.link == link => last.text.push_str(text),
            _ => self.word.push(Segment {
                text: text.to_string(),
                style,
                link,
            }),
        }
    }

    pub(crate) fn text(&mut self, s: &str) {
        let style = self.style();
        let link = self.link.clone();
        let mut run = String::new();
        for ch in s.chars() {
            if ch.is_whitespace() {
                self.append(&run, style, link.clone());
                run.clear();
                self.end_word();
                self.space();
            } else {
                run.push(ch);
            }
        }
        self.append(&run, style, link);
    }

    pub(crate) fn code(&mut self, s: &str) {
        let style = self.style().merge(self.theme.code_inline);
        let link = self.link.clone();
        let text = s.to_string();
        self.word.push(Segment { text, style, link });
    }

    pub(crate) fn soft_break(&mut self) {
        self.end_word();
        self.atoms.push(Atom::SoftBreak);
    }

    pub(crate) fn hard_break(&mut self) {
        self.end_word();
        self.atoms.push(Atom::HardBreak);
    }

    pub(crate) fn start_target(&mut self, url: &str, is_image: bool) {
        let url: Rc<str> = Rc::from(url);
        if is_image {
            self.push_style(self.theme.image);
        } else {
            self.push_style(self.theme.link);
            if self.links == LinkMode::Osc8 {
                self.link = Some(url.clone());
            }
        }
        self.targets.push((url, is_image, String::new()));
    }

    pub(crate) fn end_target(&mut self) {
        let Some((url, is_image, text)) = self.targets.pop() else {
            return;
        };
        self.pop_style();
        if !is_image {
            self.link = None;
        }
        let show_url = match (is_image, self.links) {
            (_, LinkMode::None) => false,
            (true, _) => true,
            (false, LinkMode::Inline) => text != *url && format!("mailto:{text}") != *url,
            (false, LinkMode::Osc8) => false,
        };
        if show_url {
            self.end_word();
            self.space();
            let style = self.style().merge(self.theme.link_url);
            self.word.push(Segment::new(format!("({url})"), style));
            self.end_word();
        }
    }

    pub(crate) fn finish(mut self) -> InlineText {
        self.end_word();
        InlineText(self.atoms)
    }
}

enum InlineKind {
    Paragraph { implicit: bool },
    Heading(u8),
    Cell,
}

enum Open<'t> {
    Doc(Vec<Block>),
    Quote {
        kind: Option<AlertKind>,
        blocks: Vec<Block>,
    },
    FootnoteDef {
        label: String,
        blocks: Vec<Block>,
    },
    FrontMatter {
        kind: FrontMatterKind,
        text: String,
    },
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
        tight: bool,
    },
    Item {
        task: Option<bool>,
        blocks: Vec<Block>,
    },
    Table {
        align: Vec<Align>,
        head: Vec<InlineText>,
        rows: Vec<Vec<InlineText>>,
        in_head: bool,
        row: Vec<InlineText>,
    },
    Inline {
        kind: InlineKind,
        builder: InlineBuilder<'t>,
    },
    Code {
        lang: Option<String>,
        code: String,
    },
    Html(String),
}

struct Stack<'t> {
    theme: &'t Theme,
    links: LinkMode,
    open: Vec<Open<'t>>,
    /// Footnote definitions in source order: (label, blocks).
    footnotes: Vec<(String, Vec<Block>)>,
    /// Footnote labels in order of first reference.
    referenced: Vec<String>,
}

impl Stack<'_> {
    /// Number shown for a footnote reference, assigned on first use.
    fn footnote_number(&mut self, label: &str) -> usize {
        match self.referenced.iter().position(|l| l == label) {
            Some(i) => i + 1,
            None => {
                self.referenced.push(label.to_string());
                self.referenced.len()
            }
        }
    }
}

impl<'t> Stack<'t> {
    fn top(&mut self) -> &mut Open<'t> {
        self.open.last_mut().expect("document root")
    }

    fn push_block(&mut self, block: Block) {
        match self.top() {
            Open::Doc(blocks)
            | Open::Quote { blocks, .. }
            | Open::FootnoteDef { blocks, .. }
            | Open::Item { blocks, .. } => blocks.push(block),
            _ => {
                if let Some(Open::Doc(blocks)) = self.open.first_mut() {
                    blocks.push(block);
                }
            }
        }
    }

    fn open_inline(&mut self, kind: InlineKind, base: Style) {
        let builder = InlineBuilder::new(self.theme, self.links, base);
        self.open.push(Open::Inline { kind, builder });
    }

    /// Opens an implicit paragraph when inline content arrives directly in a container.
    fn ensure_inline(&mut self) {
        if !matches!(self.top(), Open::Inline { .. }) {
            let base = self.theme.base;
            self.open_inline(InlineKind::Paragraph { implicit: true }, base);
        }
    }

    fn close_inline(&mut self) {
        let Some(Open::Inline { kind, builder }) = self.open.pop() else {
            return;
        };
        let text = builder.finish();
        match kind {
            InlineKind::Paragraph { implicit } => {
                if implicit {
                    self.mark_tight();
                }
                self.push_block(Block::Paragraph(text));
            }
            InlineKind::Heading(level) => self.push_block(Block::Heading {
                level,
                content: text,
            }),
            InlineKind::Cell => {
                if let Open::Table {
                    head, in_head, row, ..
                } = self.top()
                {
                    if *in_head {
                        head.push(text);
                    } else {
                        row.push(text);
                    }
                }
            }
        }
    }

    fn close_implicit(&mut self) {
        if matches!(
            self.top(),
            Open::Inline {
                kind: InlineKind::Paragraph { implicit: true },
                ..
            }
        ) {
            self.close_inline();
        }
    }

    fn mark_tight(&mut self) {
        for o in self.open.iter_mut().rev() {
            if let Open::List { tight, .. } = o {
                *tight = true;
                return;
            }
        }
    }

    fn builder(&mut self) -> &mut InlineBuilder<'t> {
        self.ensure_inline();
        match self.top() {
            Open::Inline { builder, .. } => builder,
            _ => unreachable!("ensure_inline opened a paragraph"),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                self.close_implicit();
                let base = self.theme.base;
                self.open_inline(InlineKind::Paragraph { implicit: false }, base);
            }
            Tag::Heading { level, .. } => {
                self.close_implicit();
                let level = heading_level(level);
                let base = self.theme.heading[usize::from(level - 1)];
                self.open_inline(InlineKind::Heading(level), base);
            }
            Tag::BlockQuote(kind) => {
                self.close_implicit();
                self.open.push(Open::Quote {
                    kind: kind.map(convert_alert),
                    blocks: Vec::new(),
                });
            }
            Tag::CodeBlock(kind) => {
                self.close_implicit();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .next()
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                    CodeBlockKind::Indented => None,
                };
                self.open.push(Open::Code {
                    lang,
                    code: String::new(),
                });
            }
            Tag::HtmlBlock => {
                self.close_implicit();
                self.open.push(Open::Html(String::new()));
            }
            Tag::List(start) => {
                self.close_implicit();
                self.open.push(Open::List {
                    start,
                    items: Vec::new(),
                    tight: false,
                });
            }
            Tag::Item => {
                self.close_implicit();
                self.open.push(Open::Item {
                    task: None,
                    blocks: Vec::new(),
                });
            }
            Tag::Table(align) => {
                self.close_implicit();
                self.open.push(Open::Table {
                    align: align.iter().map(|a| convert_align(*a)).collect(),
                    head: Vec::new(),
                    rows: Vec::new(),
                    in_head: false,
                    row: Vec::new(),
                });
            }
            Tag::TableHead => {
                if let Open::Table { in_head, .. } = self.top() {
                    *in_head = true;
                }
            }
            Tag::TableRow => {}
            Tag::TableCell => {
                let base = self.theme.base;
                self.open_inline(InlineKind::Cell, base);
            }
            Tag::Emphasis => {
                let s = self.theme.emphasis;
                self.builder().push_style(s);
            }
            Tag::Strong => {
                let s = self.theme.strong;
                self.builder().push_style(s);
            }
            Tag::Strikethrough => {
                let s = self.theme.strike;
                self.builder().push_style(s);
            }
            Tag::Superscript | Tag::Subscript => self.builder().push_style(Style::PLAIN),
            Tag::Link { dest_url, .. } => self.builder().start_target(&dest_url, false),
            Tag::Image { dest_url, .. } => self.builder().start_target(&dest_url, true),
            Tag::FootnoteDefinition(label) => {
                self.close_implicit();
                self.open.push(Open::FootnoteDef {
                    label: label.to_string(),
                    blocks: Vec::new(),
                });
            }
            Tag::MetadataBlock(kind) => {
                self.close_implicit();
                self.open.push(Open::FrontMatter {
                    kind: match kind {
                        MetadataBlockKind::YamlStyle => FrontMatterKind::Yaml,
                        MetadataBlockKind::PlusesStyle => FrontMatterKind::Toml,
                    },
                    text: String::new(),
                });
            }
            Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell => self.close_inline(),
            TagEnd::BlockQuote(_) => {
                self.close_implicit();
                if let Some(Open::Quote { kind, blocks }) = self.open.pop() {
                    self.push_block(Block::BlockQuote { kind, blocks });
                }
            }
            TagEnd::CodeBlock => {
                if let Some(Open::Code { lang, code }) = self.open.pop() {
                    self.push_block(Block::CodeBlock { lang, code });
                }
            }
            TagEnd::HtmlBlock => {
                if let Some(Open::Html(html)) = self.open.pop() {
                    self.push_block(Block::Html(html));
                }
            }
            TagEnd::List(_) => {
                self.close_implicit();
                if let Some(Open::List {
                    start,
                    items,
                    tight,
                }) = self.open.pop()
                {
                    self.push_block(Block::List {
                        start,
                        tight,
                        items,
                    });
                }
            }
            TagEnd::Item => {
                self.close_implicit();
                if let Some(Open::Item { task, blocks }) = self.open.pop()
                    && let Open::List { items, .. } = self.top()
                {
                    items.push(ListItem { task, blocks });
                }
            }
            TagEnd::Table => {
                if let Some(Open::Table {
                    align, head, rows, ..
                }) = self.open.pop()
                {
                    self.push_block(Block::Table(Table { align, head, rows }));
                }
            }
            TagEnd::TableHead => {
                if let Open::Table { in_head, .. } = self.top() {
                    *in_head = false;
                }
            }
            TagEnd::TableRow => {
                if let Open::Table { rows, row, .. } = self.top() {
                    rows.push(std::mem::take(row));
                }
            }
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript => self.builder().pop_style(),
            TagEnd::Link | TagEnd::Image => self.builder().end_target(),
            TagEnd::FootnoteDefinition => {
                self.close_implicit();
                if let Some(Open::FootnoteDef { label, blocks }) = self.open.pop() {
                    self.footnotes.push((label, blocks));
                }
            }
            TagEnd::MetadataBlock(_) => {
                if let Some(Open::FrontMatter { kind, text }) = self.open.pop() {
                    self.push_block(Block::FrontMatter { kind, text });
                }
            }
            TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle
            | TagEnd::DefinitionListDefinition => {}
        }
    }

    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(s) => match self.top() {
                Open::Code { code, .. } => code.push_str(&s),
                Open::FrontMatter { text, .. } => text.push_str(&s),
                Open::Html(html) => html.push_str(&s),
                _ => self.builder().text(&s),
            },
            Event::Code(s) => self.builder().code(&s),
            Event::InlineMath(s) | Event::DisplayMath(s) => self.builder().text(&s),
            Event::Html(s) => match self.top() {
                Open::Html(html) => html.push_str(&s),
                _ => {
                    self.close_implicit();
                    self.push_block(Block::Html(s.to_string()));
                }
            },
            Event::InlineHtml(s) => {
                for token in html::tokenize(&s) {
                    match token {
                        html::Token::Tag(tag) => self.builder().html_tag(&tag),
                        html::Token::Text(t) => self.builder().text(&html::decode_entities(&t)),
                        html::Token::Comment => {}
                    }
                }
            }
            Event::FootnoteReference(label) => {
                let number = self.footnote_number(&label);
                let style = self.theme.footnote_ref;
                let b = self.builder();
                b.push_style(style);
                b.text(&format!("[{number}]"));
                b.pop_style();
            }
            Event::SoftBreak => self.builder().soft_break(),
            Event::HardBreak => self.builder().hard_break(),
            Event::Rule => {
                self.close_implicit();
                self.push_block(Block::Rule);
            }
            Event::TaskListMarker(done) => {
                for o in self.open.iter_mut().rev() {
                    if let Open::Item { task, .. } = o {
                        *task = Some(done);
                        break;
                    }
                }
            }
        }
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn convert_align(a: Alignment) -> Align {
    match a {
        Alignment::None | Alignment::Left => Align::Left,
        Alignment::Center => Align::Center,
        Alignment::Right => Align::Right,
    }
}

/// Parses `md` into a document. Styles from `theme` are attached to inline segments here.
pub fn parse(md: &str, theme: &Theme, links: LinkMode) -> Document {
    let mut stack = Stack {
        theme,
        links,
        open: vec![Open::Doc(Vec::new())],
        footnotes: Vec::new(),
        referenced: Vec::new(),
    };
    for ev in Parser::new_ext(md, parser_options()) {
        stack.event(ev);
    }
    while stack.open.len() > 1 {
        stack.close_inline();
        if stack.open.len() > 1 && !matches!(stack.top(), Open::Inline { .. }) {
            stack.open.pop();
        }
    }
    let blocks = match stack.open.pop() {
        Some(Open::Doc(blocks)) => blocks,
        _ => Vec::new(),
    };
    // Definitions never referenced are appended after the referenced ones.
    let mut order = stack.referenced;
    for (label, _) in &stack.footnotes {
        if !order.contains(label) {
            order.push(label.clone());
        }
    }
    let footnotes = order
        .iter()
        .filter_map(|label| {
            stack
                .footnotes
                .iter()
                .find(|(l, _)| l == label)
                .map(|(_, blocks)| blocks.clone())
        })
        .collect();
    Document { blocks, footnotes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_plain(md: &str) -> Vec<Block> {
        parse(md, &Theme::default(), LinkMode::Inline).blocks
    }

    fn words(text: &InlineText) -> Vec<String> {
        text.0
            .iter()
            .map(|a| match a {
                Atom::Word(segs) => segs.iter().map(|s| s.text.as_str()).collect(),
                Atom::Space => "_".into(),
                Atom::SoftBreak => "/".into(),
                Atom::HardBreak => "//".into(),
            })
            .collect()
    }

    #[test]
    fn soft_breaks_are_kept_as_atoms() {
        let blocks = parse_plain("one two\nthree  \nfour");
        let Block::Paragraph(t) = &blocks[0] else {
            panic!()
        };
        assert_eq!(words(t), ["one", "_", "two", "/", "three", "//", "four"]);
    }

    #[test]
    fn styled_word_is_one_atom() {
        let blocks = parse_plain("**bo**ld x");
        let Block::Paragraph(t) = &blocks[0] else {
            panic!()
        };
        assert_eq!(words(t), ["bold", "_", "x"]);
        let Atom::Word(segs) = &t.0[0] else { panic!() };
        assert_eq!(segs.len(), 2);
        assert!(segs[0].style.bold && !segs[1].style.bold);
    }

    #[test]
    fn tight_and_loose_lists() {
        let tight = parse_plain("- a\n- b\n");
        let Block::List {
            tight: t, items, ..
        } = &tight[0]
        else {
            panic!()
        };
        assert!(*t);
        assert_eq!(items.len(), 2);
        let loose = parse_plain("- a\n\n- b\n");
        let Block::List { tight: t, .. } = &loose[0] else {
            panic!()
        };
        assert!(!*t);
    }

    #[test]
    fn task_markers_and_nested_lists() {
        let blocks = parse_plain("- [x] done\n  - [ ] sub\n");
        let Block::List { items, .. } = &blocks[0] else {
            panic!()
        };
        assert_eq!(items[0].task, Some(true));
        let Block::List { items: sub, .. } = &items[0].blocks[1] else {
            panic!()
        };
        assert_eq!(sub[0].task, Some(false));
    }

    #[test]
    fn inline_link_shows_url_once() {
        let blocks = parse_plain("[text](http://x) <http://y>");
        let Block::Paragraph(t) = &blocks[0] else {
            panic!()
        };
        assert_eq!(words(t), ["text", "_", "(http://x)", "_", "http://y"]);
    }

    #[test]
    fn table_shape() {
        let blocks = parse_plain("| a | b |\n|---|--:|\n| 1 | 2 |\n");
        let Block::Table(t) = &blocks[0] else {
            panic!()
        };
        assert_eq!(t.head.len(), 2);
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.align[1], Align::Right);
    }

    #[test]
    fn alerts_carry_their_kind() {
        let blocks = parse_plain("> [!WARNING]\n> careful\n\n> plain\n");
        let Block::BlockQuote { kind, .. } = &blocks[0] else {
            panic!()
        };
        assert_eq!(*kind, Some(AlertKind::Warning));
        let Block::BlockQuote { kind, .. } = &blocks[1] else {
            panic!()
        };
        assert_eq!(*kind, None);
    }

    #[test]
    fn footnotes_are_numbered_by_first_reference() {
        let doc = parse(
            "b[^b] a[^a] b[^b]\n\n[^a]: first def\n[^b]: second def\n[^c]: unused\n",
            &Theme::default(),
            LinkMode::Inline,
        );
        let Block::Paragraph(t) = &doc.blocks[0] else {
            panic!()
        };
        assert_eq!(words(t), ["b[1]", "_", "a[2]", "_", "b[1]"]);
        assert_eq!(doc.footnotes.len(), 3);
        let Block::Paragraph(first) = &doc.footnotes[0][0] else {
            panic!()
        };
        assert_eq!(words(first), ["second", "_", "def"]);
        let Block::Paragraph(last) = &doc.footnotes[2][0] else {
            panic!()
        };
        assert_eq!(words(last), ["unused"]);
    }

    #[test]
    fn front_matter_is_a_block() {
        let blocks = parse_plain("---\ntitle: x\n---\n\nbody\n");
        let Block::FrontMatter { kind, text } = &blocks[0] else {
            panic!("{blocks:?}")
        };
        assert_eq!(*kind, FrontMatterKind::Yaml);
        assert_eq!(text, "title: x\n");
        let blocks = parse_plain("+++\ntitle = \"x\"\n+++\n\nbody\n");
        let Block::FrontMatter { kind, .. } = &blocks[0] else {
            panic!("{blocks:?}")
        };
        assert_eq!(*kind, FrontMatterKind::Toml);
    }

    #[test]
    fn code_block_language() {
        let blocks = parse_plain("```rust,ignore\nfn x() {}\n```\n");
        let Block::CodeBlock { lang, code } = &blocks[0] else {
            panic!()
        };
        assert_eq!(lang.as_deref(), Some("rust"));
        assert_eq!(code, "fn x() {}\n");
    }
}
