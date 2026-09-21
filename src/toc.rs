//! Table of contents generation and insertion between HTML comment markers.

use crate::model::Document;
use crate::slug::{self, Entry};

pub mod defaults {
    /// Shallowest heading level listed; the document title is usually the only level 1.
    pub const MIN_DEPTH: u8 = 2;
    pub const MAX_DEPTH: u8 = 4;
    pub const BULLET: char = '-';
    /// Name used in the opening and closing markers.
    pub const MARKER: &str = "toc";
    /// Columns of indentation per level.
    pub const INDENT: usize = 2;
}

#[derive(Clone, Debug)]
pub struct TocOptions {
    pub min_depth: u8,
    pub max_depth: u8,
    pub bullet: char,
    pub marker: String,
}

impl Default for TocOptions {
    fn default() -> TocOptions {
        TocOptions {
            min_depth: defaults::MIN_DEPTH,
            max_depth: defaults::MAX_DEPTH,
            bullet: defaults::BULLET,
            marker: defaults::MARKER.to_string(),
        }
    }
}

impl TocOptions {
    pub fn open_marker(&self) -> String {
        format!("<!-- {} -->", self.marker)
    }

    pub fn close_marker(&self) -> String {
        format!("<!-- /{} -->", self.marker)
    }
}

/// Markdown list of the headings within the depth range, as nested links.
pub fn generate(doc: &Document, opts: &TocOptions) -> String {
    let entries: Vec<Entry> = slug::outline(doc)
        .into_iter()
        .filter(|e| e.level >= opts.min_depth && e.level <= opts.max_depth)
        .collect();
    let top = entries
        .iter()
        .map(|e| e.level)
        .min()
        .unwrap_or(opts.min_depth);
    let mut out = String::new();
    for entry in &entries {
        let indent = usize::from(entry.level - top) * defaults::INDENT;
        let text = escape_link_text(&entry.text);
        out.push_str(&" ".repeat(indent));
        out.push(opts.bullet);
        out.push_str(&format!(" [{}](#{})\n", text, entry.anchor));
    }
    out
}

/// Escapes the characters that would break a link label.
fn escape_link_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '[' || c == ']' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Where the marker block sits in the source: the line ranges to keep around it.
struct Markers {
    /// Byte offset just after the opening marker line.
    body_start: usize,
    /// Byte offset of the closing marker line.
    body_end: usize,
}

fn find_markers(src: &str, opts: &TocOptions) -> Option<Markers> {
    let open = opts.open_marker();
    let close = opts.close_marker();
    let open_at = src.lines().scan(0usize, |pos, line| {
        let start = *pos;
        *pos += line.len() + 1;
        Some((start, line))
    });
    let mut body_start = None;
    for (start, line) in open_at {
        let trimmed = line.trim();
        if body_start.is_none() && trimmed == open {
            body_start = Some(start + line.len() + 1);
        } else if body_start.is_some() && trimmed == close {
            return Some(Markers {
                body_start: body_start.expect("set above"),
                body_end: start,
            });
        }
    }
    None
}

/// Text of the document with the marker block filled with `toc`.
/// Returns `None` when the markers are missing.
pub fn insert(src: &str, toc: &str, opts: &TocOptions) -> Option<String> {
    let markers = find_markers(src, opts)?;
    let mut out = String::with_capacity(src.len() + toc.len());
    out.push_str(&src[..markers.body_start]);
    if !toc.is_empty() {
        out.push('\n');
        out.push_str(toc);
        out.push('\n');
    }
    out.push_str(&src[markers.body_end..]);
    Some(out)
}

/// Source with the marker block emptied, so headings inside it never reach the outline.
pub fn without_toc(src: &str, opts: &TocOptions) -> String {
    match find_markers(src, opts) {
        Some(m) => {
            let mut out = String::with_capacity(src.len());
            out.push_str(&src[..m.body_start]);
            out.push_str(&src[m.body_end..]);
            out
        }
        None => src.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::RenderOptions;

    fn doc(md: &str) -> Document {
        crate::document(md, &RenderOptions::plain(80))
    }

    #[test]
    fn nested_list_skips_the_title() {
        let d = doc("# Title\n\n## One\n\n### Deep\n\n#### Deeper\n\n##### Too deep\n\n## Two\n");
        let toc = generate(&d, &TocOptions::default());
        assert_eq!(
            toc,
            "- [One](#one)\n  - [Deep](#deep)\n    - [Deeper](#deeper)\n- [Two](#two)\n"
        );
    }

    #[test]
    fn brackets_in_a_heading_are_escaped() {
        let d = doc("## A [b] c\n");
        assert_eq!(
            generate(&d, &TocOptions::default()),
            "- [A \\[b\\] c](#a-b-c)\n"
        );
    }

    #[test]
    fn insertion_replaces_the_marker_body() {
        let opts = TocOptions::default();
        let src = "# T\n\n<!-- toc -->\nold\ncontent\n<!-- /toc -->\n\n## One\n";
        let out = insert(src, "- [One](#one)\n", &opts).expect("markers found");
        assert_eq!(
            out,
            "# T\n\n<!-- toc -->\n\n- [One](#one)\n\n<!-- /toc -->\n\n## One\n"
        );
        // Inserting again is stable.
        assert_eq!(
            insert(&out, "- [One](#one)\n", &opts).expect("markers"),
            out
        );
        assert!(insert("# T\n", "x", &opts).is_none());
    }

    #[test]
    fn headings_inside_the_markers_are_ignored() {
        let opts = TocOptions::default();
        let src = "<!-- toc -->\n## Stale\n<!-- /toc -->\n\n## Real\n";
        let stripped = without_toc(src, &opts);
        let toc = generate(&doc(&stripped), &opts);
        assert_eq!(toc, "- [Real](#real)\n");
    }
}
