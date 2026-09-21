//! Turning a paragraph of the ranged tree back into unbreakable words.
//!
//! Every byte of the source is accounted for: a leaf event covers it, an inline container's
//! delimiters cover it, or it is a gap. A gap is an escape backslash, whitespace before a
//! break, or a container prefix after one. Gaps are glued to the next word so no line can
//! ever break inside markup.

use crate::tree::{Kind, Node};
use pulldown_cmark::Event;

/// The spelling of a hard line break in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HardKind {
    /// Two or more trailing spaces.
    Spaces,
    /// A trailing backslash.
    Backslash,
}

impl HardKind {
    /// Text that must be written at the end of the line.
    pub fn suffix(self) -> &'static str {
        match self {
            HardKind::Spaces => "  ",
            HardKind::Backslash => "\\",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Atom {
    /// A run that must stay on one line.
    Word(String),
    /// A break opportunity between words.
    Space,
    HardBreak(HardKind),
    /// A leaf spanning several lines, kept as written. The first line joins the current
    /// line and the last one ends it.
    Verbatim(Vec<String>),
}

/// Collects the atoms of an inline container, in document order.
struct Tiler<'s> {
    src: &'s str,
    atoms: Vec<Atom>,
    /// Bytes seen but not yet attached to a word.
    glue: String,
    /// Offset up to which the source has been consumed.
    pos: usize,
    /// Set after a break, when the next gap holds container prefixes.
    after_break: bool,
}

impl<'s> Tiler<'s> {
    /// Appends `text` to the open word, or starts one.
    fn push_word(&mut self, text: &str) {
        let text = format!("{}{text}", std::mem::take(&mut self.glue));
        if text.is_empty() {
            return;
        }
        match self.atoms.last_mut() {
            Some(Atom::Word(word)) => word.push_str(&text),
            _ => self.atoms.push(Atom::Word(text)),
        }
    }

    fn push_space(&mut self) {
        if matches!(self.atoms.last(), Some(Atom::Word(_) | Atom::Verbatim(_))) {
            self.atoms.push(Atom::Space);
        }
    }

    /// Attaches `text` to the word before the trailing spaces, folding them in.
    /// Used for a closing delimiter such as `](url)` after `[foo ]`.
    fn push_closing(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut folded = String::new();
        while matches!(self.atoms.last(), Some(Atom::Space)) {
            self.atoms.pop();
            folded = " ".to_string();
        }
        if matches!(self.atoms.last(), Some(Atom::Word(_))) {
            self.push_word(&format!("{folded}{text}"));
        } else {
            self.push_word(text);
        }
    }

    /// Consumes the bytes between the cursor and `to`.
    fn gap(&mut self, to: usize) {
        if to <= self.pos {
            self.pos = self.pos.max(to);
            return;
        }
        let mut text = &self.src[self.pos..to];
        self.pos = to;
        if self.after_break {
            // Only container prefixes and indentation live here.
            text = text.trim_start_matches([' ', '\t', '>']);
            self.after_break = false;
        }
        if text.is_empty() {
            return;
        }
        if text.starts_with(char::is_whitespace) {
            self.push_space();
        }
        let rest = text.trim();
        if !rest.is_empty() {
            self.glue.push_str(rest);
        }
        if text.ends_with(char::is_whitespace) && self.glue.is_empty() {
            self.push_space();
        }
    }

    /// Splits source text on whitespace, keeping the break opportunities.
    fn text(&mut self, text: &str) {
        let mut rest = text;
        if rest.starts_with(char::is_whitespace) {
            if !self.glue.is_empty() {
                self.push_word("");
            }
            self.push_space();
            rest = rest.trim_start();
        }
        let trailing = rest.ends_with(char::is_whitespace);
        let mut first = true;
        for word in rest.split_whitespace() {
            if !first {
                self.push_space();
            }
            self.push_word(word);
            first = false;
        }
        if trailing {
            self.push_space();
        }
    }

    /// A leaf kept verbatim, which may span several lines.
    fn verbatim(&mut self, text: &str, strip: &dyn Fn(&str) -> String) {
        if !text.contains('\n') {
            self.push_word(text);
            return;
        }
        let mut lines = text.lines();
        let first = lines.next().unwrap_or_default();
        let glue = std::mem::take(&mut self.glue);
        let mut block = vec![format!("{glue}{first}")];
        block.extend(lines.map(strip));
        // A word touching the leaf has no break opportunity, so it joins the first line.
        if let Some(Atom::Word(word)) = self.atoms.pop_if(|a| matches!(a, Atom::Word(_))) {
            block[0] = format!("{word}{}", block[0]);
        }
        self.atoms.push(Atom::Verbatim(block));
    }

    fn walk(&mut self, node: &Node<'_>, strip: &dyn Fn(&str) -> String) {
        match &node.kind {
            Kind::Inline(_) => {
                self.gap(node.range.start);
                // The opening delimiter is the gap before the first child.
                for child in &node.children {
                    self.walk(child, strip);
                }
                if node.children.is_empty() {
                    self.pos = node.range.start;
                }
                let closing = &self.src[self.pos.min(node.range.end)..node.range.end];
                let closing = closing.to_string();
                self.pos = node.range.end;
                self.push_closing(&closing);
            }
            Kind::Leaf(event) => self.leaf(event, node, strip),
            _ => {
                for child in &node.children {
                    self.walk(child, strip);
                }
            }
        }
    }

    fn leaf(&mut self, event: &Event<'_>, node: &Node<'_>, strip: &dyn Fn(&str) -> String) {
        match event {
            // The marker is written by the list emitter, so it is consumed here.
            Event::TaskListMarker(_) => self.pos = self.pos.max(node.range.end),
            Event::SoftBreak => {
                self.gap(node.range.start);
                self.push_word("");
                self.push_space();
                self.pos = node.range.end;
                self.after_break = true;
            }
            Event::HardBreak => {
                self.gap(node.range.start);
                self.push_word("");
                let kind = if node.text(self.src).starts_with('\\') {
                    HardKind::Backslash
                } else {
                    HardKind::Spaces
                };
                while matches!(self.atoms.last(), Some(Atom::Space)) {
                    self.atoms.pop();
                }
                self.atoms.push(Atom::HardBreak(kind));
                self.pos = node.range.end;
                self.after_break = true;
            }
            Event::Text(_) => {
                self.gap(node.range.start);
                let text = node.text(self.src).to_string();
                self.text(&text);
                self.pos = node.range.end;
            }
            _ => {
                self.gap(node.range.start);
                let text = node.text(self.src).to_string();
                self.verbatim(&text, strip);
                self.pos = node.range.end;
            }
        }
    }
}

/// Atoms of `node`, an inline container of the tree. `strip` removes the container prefix
/// from the continuation lines of a multi-line leaf.
pub fn tile(src: &str, node: &Node<'_>, strip: &dyn Fn(&str) -> String) -> Vec<Atom> {
    // A task marker precedes the text and belongs to the list emitter, not to the words.
    let start = node
        .children
        .iter()
        .find(|c| !matches!(&c.kind, Kind::Leaf(Event::TaskListMarker(_))))
        .or(node.children.first())
        .map(|c| c.range.start)
        .unwrap_or(node.range.start);
    let mut tiler = Tiler {
        src,
        atoms: Vec::new(),
        glue: String::new(),
        pos: start,
        after_break: false,
    };
    for child in &node.children {
        tiler.walk(child, strip);
    }
    tiler.push_word("");
    while matches!(tiler.atoms.last(), Some(Atom::Space)) {
        tiler.atoms.pop();
    }
    tiler.atoms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree;

    /// Tiles the first paragraph of `src` and renders the atoms as a readable string.
    fn atoms(src: &str) -> String {
        let tree = tree::parse(src);
        let para = find_inline(&tree).expect("a paragraph");
        tile(src, para, &|line| line.trim_start().to_string())
            .iter()
            .map(|a| match a {
                Atom::Word(w) => format!("<{w}>"),
                Atom::Space => "_".to_string(),
                Atom::HardBreak(HardKind::Spaces) => "[br]".to_string(),
                Atom::HardBreak(HardKind::Backslash) => "[br\\]".to_string(),
                Atom::Verbatim(lines) => format!("<<{}>>", lines.join("|")),
            })
            .collect()
    }

    fn find_inline<'a, 'b>(node: &'a Node<'b>) -> Option<&'a Node<'b>> {
        if matches!(node.kind, Kind::Paragraph { .. } | Kind::Heading { .. }) {
            return Some(node);
        }
        node.children.iter().find_map(find_inline)
    }

    #[test]
    fn plain_words() {
        assert_eq!(atoms("one two  three\n"), "<one>_<two>_<three>");
    }

    #[test]
    fn soft_break_is_a_space() {
        assert_eq!(atoms("one\ntwo\n"), "<one>_<two>");
    }

    #[test]
    fn emphasis_delimiters_stay_with_the_word() {
        assert_eq!(atoms("a *b c* d\n"), "<a>_<*b>_<c*>_<d>");
        assert_eq!(atoms("**bo**ld x\n"), "<**bo**ld>_<x>");
    }

    #[test]
    fn a_link_destination_never_breaks() {
        assert_eq!(
            atoms("see [the docs](http://x \"t\") end\n"),
            "<see>_<[the>_<docs](http://x \"t\")>_<end>"
        );
        assert_eq!(atoms("[foo ](u)\n"), "<[foo ](u)>");
        assert_eq!(atoms("![](i.png)\n"), "<![](i.png)>");
    }

    #[test]
    fn escapes_and_entities_are_kept() {
        assert_eq!(atoms("a \\* b &amp; c\n"), "<a>_<\\*>_<b>_<&amp;>_<c>");
    }

    #[test]
    fn code_spans_are_one_word() {
        assert_eq!(atoms("a `x  y` b\n"), "<a>_<`x  y`>_<b>");
    }

    #[test]
    fn hard_breaks_keep_their_spelling() {
        assert_eq!(atoms("a  \nb\\\nc\n"), "<a>[br]<b>[br\\]<c>");
    }

    #[test]
    fn container_prefixes_are_dropped() {
        assert_eq!(atoms("> one\n> two\n"), "<one>_<two>");
        assert_eq!(atoms("- one\n  two\n"), "<one>_<two>");
    }

    #[test]
    fn a_task_marker_is_not_part_of_the_text() {
        assert_eq!(atoms("- [x] done task\n"), "<done>_<task>");
    }

    #[test]
    fn a_multi_line_code_span_stays_verbatim() {
        assert_eq!(atoms("a `x\ny` b\n"), "<a>_<<`x|y`>>_<b>");
        // With no space, the neighbouring word joins the first line.
        assert_eq!(atoms("a`x\ny`b\n"), "<<a`x|y`>><b>");
    }
}
