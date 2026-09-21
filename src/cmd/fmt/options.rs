//! Formatting options.

use crate::cmd::fmt::wrap::UNLIMITED;

pub mod defaults {
    pub const WIDTH: usize = 80;
    pub const BULLET: char = '-';
    pub const RULE: &str = "---";
    /// Spaces on each side of a table cell.
    pub const TABLE_PADDING: usize = 1;
}

#[derive(Clone, Debug)]
pub struct FmtOptions {
    /// Target line width; `UNLIMITED` puts each paragraph on one line.
    pub width: usize,
    pub bullet: char,
    pub rule: String,
    pub table_padding: usize,
}

impl Default for FmtOptions {
    fn default() -> FmtOptions {
        FmtOptions {
            width: defaults::WIDTH,
            bullet: defaults::BULLET,
            rule: defaults::RULE.to_string(),
            table_padding: defaults::TABLE_PADDING,
        }
    }
}

impl FmtOptions {
    /// Options wrapping at `width`, where zero means no wrapping.
    pub fn with_width(width: usize) -> FmtOptions {
        FmtOptions {
            width: if width == 0 { UNLIMITED } else { width },
            ..FmtOptions::default()
        }
    }
}
