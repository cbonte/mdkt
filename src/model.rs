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

/// Kind of a GitHub alert, introduced by `> [!NOTE]` and friends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    /// Label shown on the first line of the alert.
    pub fn label(self) -> &'static str {
        match self {
            AlertKind::Note => "NOTE",
            AlertKind::Tip => "TIP",
            AlertKind::Important => "IMPORTANT",
            AlertKind::Warning => "WARNING",
            AlertKind::Caution => "CAUTION",
        }
    }

    /// Index of the alert's style slot in the theme.
    pub fn index(self) -> usize {
        match self {
            AlertKind::Note => 0,
            AlertKind::Tip => 1,
            AlertKind::Important => 2,
            AlertKind::Warning => 3,
            AlertKind::Caution => 4,
        }
    }
}

/// Syntax of a metadata block at the top of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontMatterKind {
    Yaml,
    Toml,
}

impl FrontMatterKind {
    /// Highlighting language of the block content.
    pub fn language(self) -> &'static str {
        match self {
            FrontMatterKind::Yaml => "yaml",
            FrontMatterKind::Toml => "toml",
        }
    }
}

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
    BlockQuote {
        kind: Option<AlertKind>,
        blocks: Vec<Block>,
    },
    List {
        start: Option<u64>,
        tight: bool,
        items: Vec<ListItem>,
    },
    Table(Table),
    Rule,
    Html(String),
    FrontMatter {
        kind: FrontMatterKind,
        text: String,
    },
}

/// A parsed document: its blocks and the footnote definitions in reference order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
    /// Footnote bodies, indexed by the number shown in the text minus one.
    pub footnotes: Vec<Vec<Block>>,
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
