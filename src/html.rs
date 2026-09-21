//! Rendering of the HTML subset GitHub displays: a tokenizer, entity decoding, and a
//! converter turning raw HTML blocks into regular blocks.
//!
//! CommonMark splits block HTML at blank lines, so an element can open in one HTML block and
//! close in a later one with Markdown blocks in between. The converter keeps its open
//! containers across sibling blocks to reassemble them.

use crate::model::{Align, Atom, Block, InlineText, ListItem, Segment, Table};
use crate::options::LinkMode;
use crate::parse::InlineBuilder;
use crate::style::{Style, Theme};

/// An opening, closing or self-closing tag. Names are lowercase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub closing: bool,
    pub self_closing: bool,
}

impl Tag {
    /// Attribute value with entities decoded.
    pub fn attr(&self, name: &str) -> Option<String> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| decode_entities(v))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Text(String),
    Tag(Tag),
    Comment,
}

/// Elements whose content is never rendered.
const RAW_HIDDEN: [&str; 3] = ["script", "style", "template"];

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':'
}

fn parse_tag(s: &str) -> Option<(Tag, usize)> {
    let bytes = s.as_bytes();
    let mut i = 1;
    let closing = bytes.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    let start = i;
    while i < s.len() && is_name_char(s[i..].chars().next()?) {
        i += 1;
    }
    if i == start {
        return None;
    }
    let name = s[start..i].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        let rest = &s[i..];
        let trimmed = rest.trim_start();
        i += rest.len() - trimmed.len();
        let c = s[i..].chars().next()?;
        if c == '>' {
            i += 1;
            break;
        }
        if c == '/' {
            self_closing = true;
            i += 1;
            continue;
        }
        let start = i;
        while i < s.len() {
            let c = s[i..].chars().next()?;
            if c.is_whitespace() || c == '=' || c == '>' || c == '/' {
                break;
            }
            i += c.len_utf8();
        }
        if i == start {
            i += c.len_utf8();
            continue;
        }
        let attr_name = s[start..i].to_ascii_lowercase();
        let rest = &s[i..];
        let trimmed = rest.trim_start();
        let mut value = String::new();
        if trimmed.starts_with('=') {
            i += rest.len() - trimmed.len() + 1;
            let rest = &s[i..];
            let trimmed = rest.trim_start();
            i += rest.len() - trimmed.len();
            match s[i..].chars().next()? {
                q @ ('"' | '\'') => {
                    i += 1;
                    let end = s[i..].find(q)?;
                    value = s[i..i + end].to_string();
                    i += end + 1;
                }
                _ => {
                    let start = i;
                    while i < s.len() {
                        let c = s[i..].chars().next()?;
                        if c.is_whitespace() || c == '>' {
                            break;
                        }
                        i += c.len_utf8();
                    }
                    value = s[start..i].to_string();
                }
            }
        }
        attrs.push((attr_name, value));
    }
    Some((
        Tag {
            name,
            attrs,
            closing,
            self_closing,
        },
        i,
    ))
}

/// Splits HTML source into text, tags and comments. Malformed markup stays as text.
pub fn tokenize(s: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if !rest.starts_with('<') {
            let c = rest.chars().next().expect("non-empty");
            text.push(c);
            i += c.len_utf8();
            continue;
        }
        let consumed = if rest.starts_with("<!--") {
            let end = rest.find("-->").map(|e| e + 3).unwrap_or(rest.len());
            Some((None, end))
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            Some((None, rest.find('>').map(|e| e + 1).unwrap_or(rest.len())))
        } else {
            parse_tag(rest).map(|(tag, n)| (Some(tag), n))
        };
        match consumed {
            None => {
                text.push('<');
                i += 1;
            }
            Some((tag, n)) => {
                if !text.is_empty() {
                    tokens.push(Token::Text(std::mem::take(&mut text)));
                }
                i += n;
                match tag {
                    None => tokens.push(Token::Comment),
                    Some(tag) if !tag.closing && RAW_HIDDEN.contains(&tag.name.as_str()) => {
                        let close = format!("</{}", tag.name);
                        let lower = s[i..].to_ascii_lowercase();
                        match lower.find(&close) {
                            Some(pos) => {
                                let after = s[i + pos..].find('>').map(|e| e + 1).unwrap_or(0);
                                i += pos + after;
                            }
                            None => i = s.len(),
                        }
                    }
                    Some(tag) => tokens.push(Token::Tag(tag)),
                }
            }
        }
    }
    if !text.is_empty() {
        tokens.push(Token::Text(text));
    }
    tokens
}

const NAMED_ENTITIES: [(&str, char); 22] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", '\u{a0}'),
    ("copy", '\u{a9}'),
    ("reg", '\u{ae}'),
    ("trade", '\u{2122}'),
    ("hellip", '\u{2026}'),
    ("mdash", '\u{2014}'),
    ("ndash", '\u{2013}'),
    ("laquo", '\u{ab}'),
    ("raquo", '\u{bb}'),
    ("ldquo", '\u{201c}'),
    ("rdquo", '\u{201d}'),
    ("lsquo", '\u{2018}'),
    ("rsquo", '\u{2019}'),
    ("bull", '\u{2022}'),
    ("middot", '\u{b7}'),
    ("times", '\u{d7}'),
    ("rarr", '\u{2192}'),
];

/// Replaces named and numeric character references. Unknown ones are kept verbatim.
pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let Some(end) = rest[1..].find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let body = &rest[1..1 + end];
        let decoded = if let Some(num) = body.strip_prefix('#') {
            let parsed = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => num.parse().ok(),
            };
            parsed.and_then(char::from_u32)
        } else {
            NAMED_ENTITIES
                .iter()
                .find(|(n, _)| *n == body)
                .map(|(_, c)| *c)
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[2 + end..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Style applied by an inline formatting tag, `None` for tags without visual effect.
pub fn inline_style(name: &str, theme: &Theme) -> Option<Style> {
    Some(match name {
        "b" | "strong" | "mark" => theme.strong,
        "i" | "em" | "cite" | "var" | "dfn" | "q" => theme.emphasis,
        "u" | "ins" => Style::PLAIN.underline(),
        "s" | "del" | "strike" => theme.strike,
        "code" | "kbd" | "samp" | "tt" => theme.code_inline,
        "small" | "sub" | "sup" | "span" | "font" | "abbr" | "time" | "label" => Style::PLAIN,
        _ => return None,
    })
}

const BLOCK_BOUNDARIES: [&str; 14] = [
    "p",
    "div",
    "center",
    "section",
    "article",
    "header",
    "footer",
    "main",
    "nav",
    "aside",
    "figure",
    "figcaption",
    "dd",
    "dt",
];

enum Inline {
    Paragraph,
    Heading(u8),
    Summary,
    Cell { header: bool, align: Align },
}

enum Frame {
    Root(Vec<Block>),
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<ListItem>,
    },
    Item(Vec<Block>),
    Details {
        summary: Option<InlineText>,
        blocks: Vec<Block>,
    },
    Table {
        align: Vec<Align>,
        head: Vec<InlineText>,
        rows: Vec<Vec<InlineText>>,
        row: Vec<InlineText>,
        in_head: bool,
        row_is_header: bool,
    },
}

impl Frame {
    fn tag_name(&self) -> &'static str {
        match self {
            Frame::Root(_) => "",
            Frame::Quote(_) => "blockquote",
            Frame::List { .. } => "ul",
            Frame::Item(_) => "li",
            Frame::Details { .. } => "details",
            Frame::Table { .. } => "table",
        }
    }
}

struct Converter<'t> {
    theme: &'t Theme,
    links: LinkMode,
    frames: Vec<Frame>,
    inline: Option<(Inline, InlineBuilder<'t>)>,
    /// Open `<pre>`: (language, collected text).
    pre: Option<(Option<String>, String)>,
}

impl<'t> Converter<'t> {
    fn new(theme: &'t Theme, links: LinkMode) -> Converter<'t> {
        Converter {
            theme,
            links,
            frames: vec![Frame::Root(Vec::new())],
            inline: None,
            pre: None,
        }
    }

    fn top(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("root frame")
    }

    fn push_block(&mut self, block: Block) {
        match self.top() {
            Frame::Root(blocks)
            | Frame::Quote(blocks)
            | Frame::Item(blocks)
            | Frame::Details { blocks, .. } => blocks.push(block),
            Frame::List { .. } => {
                self.frames.push(Frame::Item(vec![block]));
            }
            Frame::Table { .. } => {
                self.close_frame();
                self.push_block(block);
            }
        }
    }

    fn builder(&mut self, kind: Inline) -> &mut InlineBuilder<'t> {
        if self.inline.is_none() {
            let base = match &kind {
                Inline::Heading(level) => self.theme.heading[usize::from(*level - 1)],
                Inline::Summary => self.theme.strong,
                _ => self.theme.base,
            };
            self.inline = Some((kind, InlineBuilder::new(self.theme, self.links, base)));
        }
        &mut self.inline.as_mut().expect("set above").1
    }

    fn flush_inline(&mut self) {
        let Some((kind, builder)) = self.inline.take() else {
            return;
        };
        let empty = builder.is_empty();
        let text = builder.finish();
        match kind {
            Inline::Paragraph if !empty => self.push_block(Block::Paragraph(text)),
            Inline::Paragraph => {}
            Inline::Heading(level) => self.push_block(Block::Heading {
                level,
                content: text,
            }),
            Inline::Summary => {
                for f in self.frames.iter_mut().rev() {
                    if let Frame::Details { summary, .. } = f {
                        *summary = Some(text);
                        return;
                    }
                }
                self.push_block(Block::Paragraph(text));
            }
            Inline::Cell { header, align } => {
                if let Frame::Table {
                    row,
                    row_is_header,
                    align: aligns,
                    ..
                } = self.top()
                {
                    if !header {
                        *row_is_header = false;
                    }
                    let index = row.len();
                    if aligns.len() <= index {
                        aligns.resize(index + 1, Align::Left);
                    }
                    if align != Align::Left {
                        aligns[index] = align;
                    }
                    row.push(text);
                }
            }
        }
    }

    /// Pops the top frame and appends it, as a block, to the frame below.
    fn close_frame(&mut self) {
        self.flush_inline();
        if self.frames.len() < 2 {
            return;
        }
        let frame = self.frames.pop().expect("len >= 2");
        match frame {
            Frame::Root(_) => {}
            Frame::Quote(blocks) => self.push_block(Block::BlockQuote(blocks)),
            Frame::List { start, items } => self.push_block(Block::List {
                start,
                tight: true,
                items,
            }),
            Frame::Item(blocks) => match self.top() {
                Frame::List { items, .. } => items.push(ListItem { task: None, blocks }),
                _ => self.push_block(Block::List {
                    start: None,
                    tight: true,
                    items: vec![ListItem { task: None, blocks }],
                }),
            },
            Frame::Details { summary, blocks } => {
                if let Some(summary) = summary {
                    self.push_block(Block::Paragraph(summary));
                }
                for b in blocks {
                    self.push_block(b);
                }
            }
            Frame::Table {
                mut align,
                head,
                mut rows,
                row,
                ..
            } => {
                if !row.is_empty() {
                    rows.push(row);
                }
                let table = Table {
                    align: std::mem::take(&mut align),
                    head,
                    rows,
                };
                if table.columns() > 0 {
                    self.push_block(Block::Table(table));
                }
            }
        }
    }

    /// Closes frames up to and including the innermost one opened by `name`, if any.
    fn close_named(&mut self, name: &str) {
        let name = if name == "ol" { "ul" } else { name };
        let Some(pos) = self.frames.iter().rposition(|f| f.tag_name() == name) else {
            return;
        };
        while self.frames.len() > pos {
            self.close_frame();
        }
    }

    fn open_frame(&mut self, frame: Frame) {
        self.flush_inline();
        if matches!(frame, Frame::Item(_)) {
            if matches!(self.top(), Frame::Item(_)) {
                self.close_frame();
            }
            if !matches!(self.top(), Frame::List { .. }) {
                self.frames.push(Frame::List {
                    start: None,
                    items: Vec::new(),
                });
            }
        }
        self.frames.push(frame);
    }

    fn end_row(&mut self) {
        self.flush_inline();
        if let Frame::Table {
            head,
            rows,
            row,
            in_head,
            row_is_header,
            ..
        } = self.top()
            && !row.is_empty()
        {
            let row = std::mem::take(row);
            if (*in_head || *row_is_header) && head.is_empty() && rows.is_empty() {
                *head = row;
            } else {
                rows.push(row);
            }
        }
    }

    fn pre_language(tag: &Tag) -> Option<String> {
        tag.attr("class")?
            .split_whitespace()
            .find_map(|c| {
                c.strip_prefix("language-")
                    .or_else(|| c.strip_prefix("lang-"))
            })
            .map(str::to_string)
    }

    fn tag(&mut self, tag: Tag) {
        if let Some((lang, code)) = &mut self.pre {
            match tag.name.as_str() {
                "pre" if tag.closing => {
                    let (lang, code) = self.pre.take().expect("in pre");
                    let code = code.strip_prefix('\n').unwrap_or(&code).to_string();
                    self.push_block(Block::CodeBlock { lang, code });
                }
                "code" if !tag.closing => {
                    if lang.is_none() {
                        *lang = Converter::pre_language(&tag);
                    }
                }
                "br" => code.push('\n'),
                _ => {}
            }
            return;
        }
        let name = tag.name.as_str();
        if tag.closing {
            match name {
                "blockquote" | "ul" | "ol" | "li" | "details" | "table" => self.close_named(name),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "summary" | "td" | "th" => {
                    self.flush_inline()
                }
                "tr" | "thead" | "tbody" | "tfoot" => {
                    self.end_row();
                    if name == "thead"
                        && let Frame::Table { in_head, .. } = self.top()
                    {
                        *in_head = false;
                    }
                }
                n if BLOCK_BOUNDARIES.contains(&n) => self.flush_inline(),
                _ => {
                    if let Some((_, b)) = &mut self.inline {
                        b.html_tag(&tag);
                    }
                }
            }
            return;
        }
        match name {
            "blockquote" => self.open_frame(Frame::Quote(Vec::new())),
            "ul" | "ol" => {
                let start = if name == "ol" {
                    Some(
                        tag.attr("start")
                            .and_then(|s| s.trim().parse().ok())
                            .unwrap_or(1),
                    )
                } else {
                    None
                };
                self.open_frame(Frame::List {
                    start,
                    items: Vec::new(),
                })
            }
            "li" => self.open_frame(Frame::Item(Vec::new())),
            "details" => self.open_frame(Frame::Details {
                summary: None,
                blocks: Vec::new(),
            }),
            "summary" => {
                self.flush_inline();
                self.builder(Inline::Summary);
            }
            "table" => self.open_frame(Frame::Table {
                align: Vec::new(),
                head: Vec::new(),
                rows: Vec::new(),
                row: Vec::new(),
                in_head: false,
                row_is_header: false,
            }),
            "thead" => {
                self.flush_inline();
                if let Frame::Table { in_head, .. } = self.top() {
                    *in_head = true;
                }
            }
            "tbody" | "tfoot" | "caption" | "colgroup" | "col" => self.flush_inline(),
            "tr" => {
                self.end_row();
                if let Frame::Table { row_is_header, .. } = self.top() {
                    *row_is_header = true;
                }
            }
            "td" | "th" => {
                self.flush_inline();
                if matches!(self.top(), Frame::Table { .. }) {
                    let align = match tag.attr("align").as_deref() {
                        Some("center") => Align::Center,
                        Some("right") => Align::Right,
                        _ => Align::Left,
                    };
                    self.builder(Inline::Cell {
                        header: name == "th",
                        align,
                    });
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.flush_inline();
                let level = name.as_bytes()[1] - b'0';
                self.builder(Inline::Heading(level));
            }
            "pre" => {
                self.flush_inline();
                self.pre = Some((None, String::new()));
            }
            "hr" => {
                self.flush_inline();
                self.push_block(Block::Rule);
            }
            n if BLOCK_BOUNDARIES.contains(&n) => self.flush_inline(),
            _ => self.builder(Inline::Paragraph).html_tag(&tag),
        }
    }

    fn text(&mut self, text: &str) {
        if let Some((_, code)) = &mut self.pre {
            code.push_str(&decode_entities(text));
            return;
        }
        let decoded = decode_entities(text);
        if decoded.trim().is_empty() && self.inline.is_none() {
            return;
        }
        if matches!(self.top(), Frame::Table { .. }) && self.inline.is_none() {
            return;
        }
        self.builder(Inline::Paragraph).text(&decoded);
    }

    fn html(&mut self, source: &str) {
        for token in tokenize(source) {
            match token {
                Token::Text(t) => self.text(&t),
                Token::Tag(tag) => self.tag(tag),
                Token::Comment => {}
            }
        }
    }

    fn block(&mut self, block: Block) {
        match block {
            Block::Html(source) => self.html(&source),
            Block::BlockQuote(inner) => {
                self.flush_inline();
                let inner = expand(inner, self.theme, self.links);
                self.push_block(Block::BlockQuote(inner));
            }
            Block::List {
                start,
                tight,
                items,
            } => {
                self.flush_inline();
                let items = items
                    .into_iter()
                    .map(|item| ListItem {
                        task: item.task,
                        blocks: expand(item.blocks, self.theme, self.links),
                    })
                    .collect();
                self.push_block(Block::List {
                    start,
                    tight,
                    items,
                });
            }
            other => {
                self.flush_inline();
                self.push_block(other);
            }
        }
    }

    fn finish(mut self) -> Vec<Block> {
        if let Some((lang, code)) = self.pre.take() {
            self.push_block(Block::CodeBlock { lang, code });
        }
        while self.frames.len() > 1 {
            self.close_frame();
        }
        self.flush_inline();
        match self.frames.pop() {
            Some(Frame::Root(blocks)) => blocks,
            _ => Vec::new(),
        }
    }
}

/// Replaces every `Block::Html` by the blocks its markup describes.
pub fn expand(blocks: Vec<Block>, theme: &Theme, links: LinkMode) -> Vec<Block> {
    let mut conv = Converter::new(theme, links);
    for block in blocks {
        conv.block(block);
    }
    conv.finish()
}

/// Plain text of an inline, for tests and diagnostics.
pub fn inline_to_string(text: &InlineText) -> String {
    let mut out = String::new();
    for atom in &text.0 {
        match atom {
            Atom::Word(segs) => out.extend(segs.iter().map(|s: &Segment| s.text.as_str())),
            Atom::Space => out.push(' '),
            Atom::SoftBreak => out.push('\n'),
            Atom::HardBreak => out.push('\n'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_handles_attributes_and_comments() {
        let toks = tokenize("a <a href=\"x&amp;y\" data-k=v>b</a><!-- c --><br/> <");
        assert_eq!(toks[0], Token::Text("a ".into()));
        let Token::Tag(t) = &toks[1] else { panic!() };
        assert_eq!(t.name, "a");
        assert_eq!(t.attr("href").as_deref(), Some("x&y"));
        assert_eq!(t.attr("data-k").as_deref(), Some("v"));
        assert_eq!(toks[2], Token::Text("b".into()));
        assert!(matches!(&toks[3], Token::Tag(t) if t.closing && t.name == "a"));
        assert_eq!(toks[4], Token::Comment);
        assert!(matches!(&toks[5], Token::Tag(t) if t.self_closing && t.name == "br"));
        assert_eq!(toks[6], Token::Text(" <".into()));
    }

    #[test]
    fn scripts_are_dropped() {
        let toks = tokenize("x<script>var a = '<b>';</script>y<style>p{}</style>");
        assert_eq!(toks, vec![Token::Text("x".into()), Token::Text("y".into())]);
    }

    #[test]
    fn entities() {
        assert_eq!(
            decode_entities("a &amp; b &lt;c&gt; &#65;&#x42; &unknown; &"),
            "a & b <c> AB &unknown; &"
        );
    }

    fn blocks(md: &str) -> Vec<Block> {
        let theme = Theme::default();
        let parsed = crate::parse::parse(md, &theme, LinkMode::Inline);
        expand(parsed, &theme, LinkMode::Inline)
    }

    #[test]
    fn details_spanning_markdown_blocks() {
        let b = blocks(
            "<details>\n<summary>More</summary>\n\nHidden **text**\n\n</details>\n\nAfter\n",
        );
        assert_eq!(b.len(), 3);
        let Block::Paragraph(summary) = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(inline_to_string(summary), "More");
        let Block::Paragraph(body) = &b[1] else {
            panic!("{b:?}")
        };
        assert_eq!(inline_to_string(body), "Hidden text");
    }

    #[test]
    fn html_list_and_table() {
        let b = blocks(
            "<ul>\n<li>one</li>\n<li>two <b>bold</b></li>\n</ul>\n<table>\n<tr><th>A</th><th align=\"right\">B</th></tr>\n<tr><td>1</td><td>2</td></tr>\n</table>\n",
        );
        let Block::List { items, .. } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(items.len(), 2);
        let Block::Table(t) = &b[1] else {
            panic!("{b:?}")
        };
        assert_eq!(t.head.len(), 2);
        assert_eq!(
            t.rows,
            vec![vec![
                InlineText(vec![Atom::Word(vec![Segment::new("1", Style::PLAIN)])]),
                InlineText(vec![Atom::Word(vec![Segment::new("2", Style::PLAIN)])]),
            ]]
        );
        assert_eq!(t.align[1], Align::Right);
    }

    #[test]
    fn pre_becomes_code_block() {
        let b = blocks("<pre><code class=\"language-rust\">fn a() {}\n&lt;x&gt;\n</code></pre>\n");
        let Block::CodeBlock { lang, code } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(lang.as_deref(), Some("rust"));
        assert_eq!(code, "fn a() {}\n<x>\n");
    }

    #[test]
    fn inline_tags_inside_markdown_paragraph() {
        let b = blocks("a <b>bold</b> b<br>c <img src=\"i.png\" alt=\"pic\"> <!-- x --> d\n");
        let Block::Paragraph(t) = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(inline_to_string(t), "a bold b\nc pic (i.png) d");
        let Atom::Word(w) = &t.0[2] else { panic!() };
        assert!(w[0].style.bold);
    }
}
