//! Positions and container prefixes over a Markdown source.

/// Line starts of a source, for turning byte offsets into positions.
pub struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(src: &str) -> LineIndex {
        let mut starts = vec![0];
        starts.extend(
            src.char_indices()
                .filter(|(_, c)| *c == '\n')
                .map(|(i, _)| i + 1),
        );
        LineIndex {
            starts,
            len: src.len(),
        }
    }

    /// One-based number of the line holding `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        let offset = offset.min(self.len);
        match self.starts.binary_search(&offset) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }

    /// One-based line and column of `offset`, the column counted in characters.
    pub fn line_col(&self, src: &str, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.len);
        let line = self.line_of(offset);
        let start = self.starts[line - 1];
        (line, src[start..offset].chars().count() + 1)
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// Byte range of a one-based line, newline excluded.
    pub fn line_range(&self, src: &str, line: usize) -> std::ops::Range<usize> {
        let start = self.starts[line - 1];
        let end = self
            .starts
            .get(line)
            .map(|next| next - 1)
            .unwrap_or(self.len);
        let end = if src[start..end].ends_with('\r') {
            end - 1
        } else {
            end
        };
        start..end
    }
}

/// Tab stop used when a container prefix is measured in columns.
pub const TAB_STOP: usize = 4;

/// Strips one block quote marker (`>` with an optional space) after up to three spaces.
/// Returns the rest of the line, or `None` when no marker is there.
pub fn strip_quote(line: &str) -> Option<&str> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = line[indent..].strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

/// Removes up to `columns` columns of leading whitespace, expanding tabs to the tab stop.
/// A tab straddling the boundary leaves its remaining columns as spaces.
pub fn strip_indent(line: &str, columns: usize) -> String {
    let mut column = 0;
    let mut chars = line.char_indices();
    while column < columns {
        let Some((i, c)) = chars.next() else {
            return String::new();
        };
        match c {
            ' ' => column += 1,
            '\t' => column += TAB_STOP - column % TAB_STOP,
            _ => return line[i..].to_string(),
        }
        if column >= columns {
            let rest = chars.as_str();
            return " ".repeat(column - columns) + rest;
        }
    }
    line[..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions() {
        let src = "ab\ncde\n\nx";
        let index = LineIndex::new(src);
        assert_eq!(index.line_col(src, 0), (1, 1));
        assert_eq!(index.line_col(src, 2), (1, 3));
        assert_eq!(index.line_col(src, 3), (2, 1));
        assert_eq!(index.line_col(src, 8), (4, 1));
        assert_eq!(index.line_count(), 4);
        assert_eq!(index.line_of(3), 2);
        assert_eq!(index.line_of(999), 4);
    }

    #[test]
    fn multibyte_columns_count_characters() {
        let src = "e\u{301}\u{4e2d}x";
        let index = LineIndex::new(src);
        let offset = src.find('x').expect("x present");
        assert_eq!(index.line_col(src, offset), (1, 4));
    }

    #[test]
    fn line_ranges_drop_the_carriage_return() {
        let src = "a\r\nbb\n";
        let index = LineIndex::new(src);
        assert_eq!(&src[index.line_range(src, 1)], "a");
        assert_eq!(&src[index.line_range(src, 2)], "bb");
    }

    #[test]
    fn quote_markers() {
        assert_eq!(strip_quote("> text"), Some("text"));
        assert_eq!(strip_quote(">text"), Some("text"));
        assert_eq!(strip_quote("   > text"), Some("text"));
        assert_eq!(strip_quote(">"), Some(""));
        assert_eq!(strip_quote("    > too deep"), None);
        assert_eq!(strip_quote("text"), None);
    }

    #[test]
    fn indent_stripping_expands_tabs() {
        assert_eq!(strip_indent("    text", 2), "  text");
        assert_eq!(strip_indent("\ttext", 2), "  text");
        assert_eq!(strip_indent("\ttext", 4), "text");
        assert_eq!(strip_indent("  text", 4), "text");
        assert_eq!(strip_indent("text", 2), "text");
    }
}
