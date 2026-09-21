//! GitHub heading anchors and the document outline.

use std::collections::HashMap;

use crate::model::{Atom, Block, Document, InlineText, Segment};

/// Plain text of an inline, with soft and hard breaks as spaces.
pub fn inline_text(text: &InlineText) -> String {
    let mut out = String::new();
    for atom in &text.0 {
        match atom {
            Atom::Word(segs) => out.extend(segs.iter().map(|s: &Segment| s.text.as_str())),
            Atom::Space | Atom::SoftBreak | Atom::HardBreak => out.push(' '),
        }
    }
    out.trim().to_string()
}

/// Anchor GitHub gives a heading: lower case, punctuation dropped, spaces to hyphens.
/// `seen` counts earlier anchors so duplicates get the `-1`, `-2` suffix GitHub adds.
pub fn github_anchor(text: &str, seen: &mut HashMap<String, usize>) -> String {
    let base: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect();
    let count = seen.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{}", *count - 1)
    }
}

/// One heading of the document outline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub level: u8,
    pub text: String,
    pub anchor: String,
    /// Index of the heading block among the document's top-level blocks.
    pub index: usize,
}

/// Top-level headings of the document, in order. Headings nested in quotes or list items
/// are not part of an outline, which matches what GitHub links to.
pub fn outline(doc: &Document) -> Vec<Entry> {
    let mut seen = HashMap::new();
    doc.blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| match block {
            Block::Heading { level, content } => {
                let text = inline_text(content);
                let anchor = github_anchor(&text, &mut seen);
                Some(Entry {
                    level: *level,
                    text,
                    anchor,
                    index,
                })
            }
            _ => None,
        })
        .collect()
}

/// Blocks of the section opened by `entry`: the heading and everything up to the next
/// heading of the same or a higher level.
pub fn section_range(doc: &Document, entry: &Entry) -> std::ops::Range<usize> {
    let end = doc
        .blocks
        .iter()
        .enumerate()
        .skip(entry.index + 1)
        .find(|(_, b)| matches!(b, Block::Heading { level, .. } if *level <= entry.level))
        .map(|(i, _)| i)
        .unwrap_or(doc.blocks.len());
    entry.index..end
}

/// Outline entry whose text or anchor matches `name`, ignoring case.
pub fn find_section<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    let name = name.trim();
    entries
        .iter()
        .find(|e| e.text.eq_ignore_ascii_case(name) || e.anchor.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::RenderOptions;

    fn anchors(texts: &[&str]) -> Vec<String> {
        let mut seen = HashMap::new();
        texts.iter().map(|t| github_anchor(t, &mut seen)).collect()
    }

    #[test]
    fn anchor_drops_punctuation_and_keeps_letters() {
        assert_eq!(anchors(&["Hello, World!"]), ["hello-world"]);
        assert_eq!(anchors(&["What's new in 2.0?"]), ["whats-new-in-20"]);
        assert_eq!(anchors(&["Accents: cafe cree"]), ["accents-cafe-cree"]);
        assert_eq!(
            anchors(&["snake_case and kebab-case"]),
            ["snake_case-and-kebab-case"]
        );
    }

    #[test]
    fn duplicate_anchors_are_numbered() {
        assert_eq!(
            anchors(&["Usage", "Usage", "Usage"]),
            ["usage", "usage-1", "usage-2"]
        );
    }

    fn doc(md: &str) -> Document {
        crate::document(md, &RenderOptions::plain(80))
    }

    #[test]
    fn outline_skips_nested_headings() {
        let d = doc("# One\n\ntext\n\n## Two\n\n> # Quoted\n\n- # In a list\n\n## Two\n");
        let entries = outline(&d);
        let names: Vec<&str> = entries.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(names, ["One", "Two", "Two"]);
        assert_eq!(entries[2].anchor, "two-1");
        assert_eq!(entries[0].level, 1);
    }

    #[test]
    fn section_stops_at_the_next_heading_of_the_same_level() {
        let d = doc("# A\n\na1\n\n## B\n\nb1\n\n### C\n\nc1\n\n## D\n\nd1\n");
        let entries = outline(&d);
        let b = find_section(&entries, "b").expect("B found");
        let range = section_range(&d, b);
        assert_eq!(range, 2..6);
        let a = find_section(&entries, "A").expect("A found");
        assert_eq!(section_range(&d, a), 0..d.blocks.len());
        assert!(find_section(&entries, "missing").is_none());
    }
}
