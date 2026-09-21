//! Fully resolved rendering options and their defaults.

use crate::style::{ColorLevel, Theme};
use crate::table::{ASCII_BORDERS, Borders, UNICODE_BORDERS};
use crate::wrap::SoftBreaks;

/// How links are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkMode {
    /// OSC 8 hyperlink around the link text.
    Osc8,
    /// Link text followed by the URL in parentheses.
    Inline,
    /// Link text only.
    None,
}

/// Whether a metadata block at the top of the document is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum FrontMatter {
    #[default]
    Hide,
    Show,
}

/// What to do with table cells wider than their column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum TableOverflow {
    #[default]
    Wrap,
    Truncate,
}

/// Decorative strings, one set per character repertoire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glyphs {
    pub bullet: String,
    pub quote_marker: String,
    pub rule: char,
    pub borders: Borders,
    pub ellipsis: String,
    pub task_done: String,
    pub task_todo: String,
}

impl Glyphs {
    pub fn unicode() -> Glyphs {
        Glyphs {
            bullet: "\u{2022} ".into(),
            quote_marker: "\u{2502} ".into(),
            rule: '\u{2500}',
            borders: UNICODE_BORDERS,
            ellipsis: "\u{2026}".into(),
            task_done: "[\u{2713}] ".into(),
            task_todo: "[ ] ".into(),
        }
    }

    pub fn ascii() -> Glyphs {
        Glyphs {
            bullet: "- ".into(),
            quote_marker: "> ".into(),
            rule: '-',
            borders: ASCII_BORDERS,
            ellipsis: "...".into(),
            task_done: "[x] ".into(),
            task_todo: "[ ] ".into(),
        }
    }
}

pub mod defaults {
    /// Line width when no terminal size is available.
    pub const WIDTH: usize = 80;
    pub const TAB_WIDTH: usize = 4;
    /// Spaces before each code block line.
    pub const CODE_INDENT: usize = 2;
    /// Content width kept even under deep nesting.
    pub const MIN_CONTENT_WIDTH: usize = 10;
    /// Spaces on each side of a table cell.
    pub const TABLE_PADDING: usize = 1;
    pub const CODE_THEME: &str = "base16-ocean.dark";
    /// Deepest heading level listed by an outline.
    pub const TOC_DEPTH: u8 = 3;
    /// Columns of indentation per outline level.
    pub const TOC_INDENT: usize = 2;
}

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub width: usize,
    pub color: ColorLevel,
    pub links: LinkMode,
    pub soft_breaks: SoftBreaks,
    pub theme: Theme,
    /// syntect theme name for code blocks.
    pub code_theme: String,
    pub glyphs: Glyphs,
    pub tab_width: usize,
    pub code_indent: usize,
    pub min_content_width: usize,
    pub table_padding: usize,
    pub table_overflow: TableOverflow,
    /// Show `#` markers before heading text.
    pub heading_marker: bool,
    pub front_matter: FrontMatter,
}

impl RenderOptions {
    /// Escape-free, ASCII-only rendering at `width`. Used by tests and `--color never`.
    pub fn plain(width: usize) -> RenderOptions {
        RenderOptions {
            width,
            color: ColorLevel::None,
            links: LinkMode::Inline,
            soft_breaks: SoftBreaks::Join,
            theme: Theme::default(),
            code_theme: defaults::CODE_THEME.into(),
            glyphs: Glyphs::ascii(),
            tab_width: defaults::TAB_WIDTH,
            code_indent: defaults::CODE_INDENT,
            min_content_width: defaults::MIN_CONTENT_WIDTH,
            table_padding: defaults::TABLE_PADDING,
            table_overflow: TableOverflow::Wrap,
            heading_marker: true,
            front_matter: FrontMatter::Hide,
        }
    }
}

impl Default for RenderOptions {
    fn default() -> RenderOptions {
        RenderOptions::plain(defaults::WIDTH)
    }
}
