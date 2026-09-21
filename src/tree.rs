//! A tree of source ranges over the parser events, for tools that rewrite or locate
//! Markdown rather than render it.
//!
//! Every node carries the byte range it covers, so the original source can be sliced back
//! out. The invariants the tools rely on are pinned by the tests at the end of this file.

use std::ops::Range;

use pulldown_cmark::{Alignment, CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};

/// Parser options used by the rewriting tools. Smart punctuation stays off: it would
/// replace quotes and dashes and make a rewrite lossy.
pub fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind<'a> {
    Document,
    /// `implicit` marks the paragraph of a tight list item, which emits no tag of its own.
    Paragraph {
        implicit: bool,
    },
    Heading {
        level: u8,
    },
    BlockQuote,
    List {
        start: Option<u64>,
    },
    Item,
    FootnoteDef(CowStr<'a>),
    CodeBlock(CodeBlockKind<'a>),
    HtmlBlock,
    MetadataBlock,
    Rule,
    Table(Vec<Alignment>),
    TableHead,
    TableRow,
    TableCell,
    /// An inline container: emphasis, strong, strikethrough, link or image.
    Inline(Tag<'a>),
    /// A leaf event: text, code span, break, inline HTML, footnote reference.
    Leaf(Event<'a>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node<'a> {
    pub kind: Kind<'a>,
    pub range: Range<usize>,
    pub children: Vec<Node<'a>>,
}

impl<'a> Node<'a> {
    fn new(kind: Kind<'a>, range: Range<usize>) -> Node<'a> {
        Node {
            kind,
            range,
            children: Vec::new(),
        }
    }

    /// Source text this node covers.
    pub fn text<'s>(&self, src: &'s str) -> &'s str {
        &src[self.range.clone()]
    }

    /// True when the node holds block children rather than inline content.
    pub fn is_block(&self) -> bool {
        !matches!(self.kind, Kind::Inline(_) | Kind::Leaf(_))
    }

    /// Whether a list is tight, that is whether its items hold implicit paragraphs.
    /// Meaningless on any other kind.
    pub fn is_tight(&self) -> bool {
        self.children.iter().any(|item| {
            item.children
                .iter()
                .any(|b| matches!(b.kind, Kind::Paragraph { implicit: true }))
        })
    }
}

fn block_kind<'a>(tag: Tag<'a>) -> Option<Kind<'a>> {
    Some(match tag {
        Tag::Paragraph => Kind::Paragraph { implicit: false },
        Tag::Heading { level, .. } => Kind::Heading { level: level as u8 },
        Tag::BlockQuote(_) => Kind::BlockQuote,
        Tag::List(start) => Kind::List { start },
        Tag::Item => Kind::Item,
        Tag::FootnoteDefinition(label) => Kind::FootnoteDef(label),
        Tag::CodeBlock(kind) => Kind::CodeBlock(kind),
        Tag::HtmlBlock => Kind::HtmlBlock,
        Tag::MetadataBlock(_) => Kind::MetadataBlock,
        Tag::Table(align) => Kind::Table(align),
        Tag::TableHead => Kind::TableHead,
        Tag::TableRow => Kind::TableRow,
        Tag::TableCell => Kind::TableCell,
        _ => return None,
    })
}

/// Builds the ranged tree of `src`.
pub fn parse(src: &str) -> Node<'_> {
    let mut stack = vec![Node::new(Kind::Document, 0..src.len())];
    for (event, range) in Parser::new_ext(src, parser_options()).into_offset_iter() {
        match event {
            Event::Start(tag) => match block_kind(tag.clone()) {
                // A block tag closes the tight paragraph a previous inline event opened.
                Some(kind) => {
                    close_implicit(&mut stack);
                    stack.push(Node::new(kind, range));
                }
                // An inline event arriving in a block container opens a tight paragraph.
                None => {
                    open_implicit(&mut stack, range.start);
                    stack.push(Node::new(Kind::Inline(tag), range));
                }
            },
            Event::End(tag) => {
                if matches!(
                    tag,
                    TagEnd::Paragraph
                        | TagEnd::Heading(_)
                        | TagEnd::TableCell
                        | TagEnd::FootnoteDefinition
                ) {
                    close_implicit(&mut stack);
                }
                if let Some(node) = stack.pop() {
                    if matches!(node.kind, Kind::Paragraph { implicit: true }) {
                        // The implicit paragraph was still open: close it and the real tag.
                        push_child(&mut stack, node);
                        if let Some(node) = stack.pop() {
                            push_child(&mut stack, node);
                        }
                    } else {
                        push_child(&mut stack, node);
                    }
                }
            }
            Event::Rule => {
                close_implicit(&mut stack);
                push_child(&mut stack, Node::new(Kind::Rule, range));
            }
            leaf => {
                if !matches!(leaf, Event::TaskListMarker(_)) {
                    open_implicit(&mut stack, range.start);
                }
                push_child(&mut stack, Node::new(Kind::Leaf(leaf), range));
            }
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().expect("len > 1");
        push_child(&mut stack, node);
    }
    stack.pop().expect("document root")
}

fn push_child<'a>(stack: &mut [Node<'a>], mut node: Node<'a>) {
    let Some(parent) = stack.last_mut() else {
        return;
    };
    // An implicit paragraph has no tag, so its range grows with its children.
    if matches!(parent.kind, Kind::Paragraph { implicit: true }) {
        parent.range.end = parent.range.end.max(node.range.end);
        if parent.range.start > node.range.start {
            parent.range.start = node.range.start;
        }
    }
    node.children.shrink_to_fit();
    parent.children.push(node);
}

fn open_implicit(stack: &mut Vec<Node<'_>>, start: usize) {
    let open_block = stack.last().is_some_and(|n| {
        matches!(
            n.kind,
            Kind::Document
                | Kind::BlockQuote
                | Kind::Item
                | Kind::List { .. }
                | Kind::FootnoteDef(_)
        )
    });
    if open_block {
        stack.push(Node::new(Kind::Paragraph { implicit: true }, start..start));
    }
}

fn close_implicit(stack: &mut Vec<Node<'_>>) {
    if let Some(node) = stack.pop_if(|n| matches!(n.kind, Kind::Paragraph { implicit: true })) {
        push_child(stack, node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flattens the tree into `(depth, kind label, covered text)` triples.
    fn dump(src: &str) -> Vec<(usize, String, String)> {
        fn walk(node: &Node<'_>, src: &str, depth: usize, out: &mut Vec<(usize, String, String)>) {
            // The variant name alone, without its payload.
            let name =
                |s: String| -> String { s.chars().take_while(|c| c.is_alphanumeric()).collect() };
            let label = match &node.kind {
                Kind::Leaf(e) => name(format!("{e:?}")),
                Kind::Inline(t) => name(format!("{t:?}")),
                Kind::Paragraph { implicit: true } => "TightParagraph".to_string(),
                other => name(format!("{other:?}")),
            };
            out.push((depth, label, node.text(src).to_string()));
            for child in &node.children {
                walk(child, src, depth + 1, out);
            }
        }
        let tree = parse(src);
        let mut out = Vec::new();
        for child in &tree.children {
            walk(child, src, 0, &mut out);
        }
        out
    }

    /// Ranges of every leaf event in the tree.
    fn leaf_ranges(node: &Node<'_>) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        if matches!(node.kind, Kind::Leaf(_)) {
            out.push(node.range.clone());
        }
        for child in &node.children {
            out.extend(leaf_ranges(child));
        }
        out
    }

    fn texts(src: &str, label: &str) -> Vec<String> {
        dump(src)
            .into_iter()
            .filter(|(_, l, _)| l == label)
            .map(|(_, _, t)| t)
            .collect()
    }

    #[test]
    fn text_events_tile_the_source() {
        let src = "one two three\n";
        let covered: String = texts(src, "Text").concat();
        assert_eq!(covered, "one two three");
    }

    #[test]
    fn an_escape_backslash_is_covered_by_no_event() {
        let src = "a \\* b\n";
        let at = src.find('\\').expect("backslash present");
        let covered = dump(src).into_iter().any(|(_, label, _)| {
            label == "Text" && src.find(&src[at..at + 2]).is_some_and(|_| false)
        });
        assert!(!covered);
        // The escaped star opens a text event; the backslash before it belongs to none.
        assert_eq!(texts(src, "Text").concat(), "a * b");
        let tree = parse(src);
        let leaves = leaf_ranges(&tree);
        assert!(
            !leaves.iter().any(|r| r.contains(&at)),
            "backslash at {at} covered by {leaves:?}"
        );
    }

    #[test]
    fn an_entity_range_covers_the_whole_reference() {
        let src = "a &amp; b\n";
        assert_eq!(texts(src, "Text"), ["a ", "&amp;", " b"]);
    }

    #[test]
    fn a_soft_break_covers_only_the_line_ending() {
        // One trailing space is not a hard break, and it is covered by nothing.
        let src = "one \ntwo\n";
        assert_eq!(texts(src, "SoftBreak"), ["\n"]);
        assert_eq!(texts(src, "Text"), ["one", "two"]);
    }

    #[test]
    fn a_hard_break_covers_its_spelling() {
        assert_eq!(texts("one   \ntwo\n", "HardBreak"), ["   \n"]);
        assert_eq!(texts("one\\\ntwo\n", "HardBreak"), ["\\\n"]);
    }

    #[test]
    fn container_prefixes_are_outside_text_ranges() {
        let src = "> one\n> two\n";
        assert_eq!(texts(src, "Text"), ["one", "two"]);
        let src = "- one\n  two\n";
        assert_eq!(texts(src, "Text"), ["one", "two"]);
    }

    #[test]
    fn inline_containers_cover_their_delimiters() {
        assert_eq!(texts("a *b* c\n", "Emphasis"), ["*b*"]);
        assert_eq!(texts("a **b** c\n", "Strong"), ["**b**"]);
        let src = "see [text](http://x \"t\") end\n";
        assert_eq!(texts(src, "Link"), ["[text](http://x \"t\")"]);
        assert_eq!(texts("![alt](i.png)\n", "Image"), ["![alt](i.png)"]);
    }

    #[test]
    fn a_code_span_covers_its_backticks() {
        assert_eq!(texts("a `x  y` b\n", "Code"), ["`x  y`"]);
        // The event text is normalised; only the raw slice keeps the spacing.
        assert_eq!(texts("a ``x`y`` b\n", "Code"), ["``x`y``"]);
    }

    #[test]
    fn a_footnote_reference_covers_its_marker() {
        assert_eq!(
            texts("a[^one] b\n\n[^one]: def\n", "FootnoteReference"),
            ["[^one]"]
        );
    }

    #[test]
    fn verbatim_blocks_cover_their_fences_and_markers() {
        // The range stops at the closing fence, before its line ending.
        let src = "```rust\nfn a() {}\n```\n";
        assert_eq!(texts(src, "CodeBlock"), ["```rust\nfn a() {}\n```"]);
        let src = "---\ntitle: x\n---\n\nbody\n";
        assert_eq!(texts(src, "MetadataBlock"), ["---\ntitle: x\n---"]);
    }

    #[test]
    fn reference_definitions_are_covered_by_no_block() {
        let src = "see [a][ref]\n\n[ref]: http://x\n";
        let tree = parse(src);
        let covered: Vec<Range<usize>> = tree.children.iter().map(|c| c.range.clone()).collect();
        let def_at = src.find("[ref]:").expect("definition present");
        assert!(
            !covered.iter().any(|r| r.contains(&def_at)),
            "the definition at {def_at} is inside {covered:?}"
        );
    }

    #[test]
    fn a_nested_list_leaves_the_tight_paragraph() {
        let tree = parse("- one\n  - nested\n");
        let Kind::List { .. } = tree.children[0].kind else {
            panic!()
        };
        let item = &tree.children[0].children[0];
        let kinds: Vec<&Kind<'_>> = item.children.iter().map(|c| &c.kind).collect();
        assert!(
            matches!(
                kinds.as_slice(),
                [Kind::Paragraph { implicit: true }, Kind::List { .. }]
            ),
            "{kinds:?}"
        );
    }

    #[test]
    fn tight_and_loose_items() {
        let tight = parse("- a\n- b\n");
        let Kind::List { .. } = tight.children[0].kind else {
            panic!("{:?}", tight.children[0].kind)
        };
        assert!(tight.children[0].is_tight());
        let loose = parse("- a\n\n- b\n");
        assert!(!loose.children[0].is_tight());
    }

    #[test]
    fn task_markers_precede_the_item_text() {
        let src = "- [x] done\n";
        let dumped = dump(src);
        let labels: Vec<&str> = dumped.iter().map(|(_, l, _)| l.as_str()).collect();
        assert!(labels.contains(&"TaskListMarker"), "{labels:?}");
        assert_eq!(texts(src, "Text"), ["done"]);
    }

    #[test]
    fn table_cells_cover_the_bytes_between_the_pipes() {
        let src = "| a | bb |\n|---|---|\n| 1 | 2 |\n";
        assert_eq!(texts(src, "TableCell"), [" a ", " bb ", " 1 ", " 2 "]);
    }
}
