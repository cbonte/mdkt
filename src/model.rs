//! Document model produced by the parser and consumed by the layout.

use std::rc::Rc;

use crate::style::Style;

/// A run of text with one style. Holds no newline; whitespace only inside inline code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub style: Style,
    pub link: Option<Rc<str>>,
}

impl Segment {
    pub fn new(text: impl Into<String>, style: Style) -> Segment {
        Segment {
            text: text.into(),
            style,
            link: None,
        }
    }
}

/// One output line: styled segments, no newline.
pub type Line = Vec<Segment>;

/// Inline content atom. A `Word` is never broken across lines unless wider than the line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Atom {
    Word(Vec<Segment>),
    Space,
    SoftBreak,
    HardBreak,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineText(pub Vec<Atom>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(InlineText),
    Heading {
        level: u8,
        content: InlineText,
    },
    CodeBlock {
        lang: Option<String>,
        code: String,
    },
    BlockQuote(Vec<Block>),
    List {
        start: Option<u64>,
        tight: bool,
        items: Vec<ListItem>,
    },
    Table(Table),
    Rule,
    Html(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// `Some(done)` for a task list item.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub align: Vec<Align>,
    pub head: Vec<InlineText>,
    pub rows: Vec<Vec<InlineText>>,
}

impl Table {
    pub fn columns(&self) -> usize {
        self.head
            .len()
            .max(self.rows.iter().map(Vec::len).max().unwrap_or(0))
    }
}
