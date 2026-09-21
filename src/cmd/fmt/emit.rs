//! Walking the ranged tree and writing the formatted document.

use std::ops::Range;

use crate::source::{LineIndex, strip_indent, strip_quote};
use crate::tree::{Kind, Node};
use pulldown_cmark::{Alignment, CodeBlockKind, Event};

use crate::cmd::fmt::hazards;
use crate::cmd::fmt::inline::tile;
use crate::cmd::fmt::options::FmtOptions;
use crate::cmd::fmt::table;
use crate::cmd::fmt::wrap::{UNLIMITED, wrap};

/// Content width kept whatever the nesting depth.
const MIN_CONTENT_WIDTH: usize = 20;
/// Spaces a footnote definition indents its continuation lines by.
const FOOTNOTE_INDENT: usize = 4;
/// Bullet characters cycled through so two adjacent lists do not merge.
const BULLET_CYCLE: [char; 3] = ['-', '*', '+'];

/// One open container: what to write before the first line and before the others.
struct Prefix {
    first: String,
    rest: String,
    used: bool,
}

impl Prefix {
    fn same(text: String) -> Prefix {
        Prefix {
            first: text.clone(),
            rest: text,
            used: false,
        }
    }
}

pub struct Emitter<'s> {
    src: &'s str,
    opts: &'s FmtOptions,
    index: LineIndex,
    prefixes: Vec<Prefix>,
    out: String,
    /// A blank line is owed before the next emitted line.
    blank_pending: bool,
    started: bool,
}

impl<'s> Emitter<'s> {
    pub fn new(src: &'s str, opts: &'s FmtOptions) -> Emitter<'s> {
        Emitter {
            src,
            opts,
            index: LineIndex::new(src),
            prefixes: Vec::new(),
            out: String::new(),
            blank_pending: false,
            started: false,
        }
    }

    /// Formats the whole document.
    pub fn run(mut self, root: &Node<'_>) -> String {
        let range = root.range.clone();
        self.blocks_in(Some(&range), &root.children, false);
        self.out
    }

    fn width(&self) -> usize {
        if self.opts.width == UNLIMITED {
            return UNLIMITED;
        }
        // A marker line can be wider than the continuation indent, as for a footnote.
        let used: usize = self
            .prefixes
            .iter()
            .map(|p| p.first.chars().count().max(p.rest.chars().count()))
            .sum();
        self.opts.width.saturating_sub(used).max(MIN_CONTENT_WIDTH)
    }

    /// Writes the blank line owed between two blocks, under the open prefixes.
    fn flush_blank(&mut self) {
        if self.blank_pending && self.started {
            let blank: String = self
                .prefixes
                .iter()
                .map(|p| p.rest.as_str())
                .collect::<String>();
            self.out.push_str(blank.trim_end());
            self.out.push('\n');
        }
        self.blank_pending = false;
    }

    fn line(&mut self, text: &str) {
        self.flush_blank();
        for prefix in &mut self.prefixes {
            self.out.push_str(if prefix.used {
                &prefix.rest
            } else {
                &prefix.first
            });
            prefix.used = true;
        }
        // A line with no content must not keep the trailing space of its prefix.
        if text.is_empty() {
            let end = self.out.trim_end_matches(' ').len();
            self.out.truncate(end);
        } else {
            self.out.push_str(text);
        }
        self.out.push('\n');
        self.started = true;
    }

    fn lines(&mut self, lines: &[String]) {
        for line in lines {
            self.line(line);
        }
    }

    fn with_prefix<R>(&mut self, prefix: Prefix, f: impl FnOnce(&mut Self) -> R) -> R {
        // The blank line belongs to the enclosing container, not to this one.
        self.flush_blank();
        self.prefixes.push(prefix);
        let result = f(self);
        let prefix = self.prefixes.pop().expect("pushed above");
        // A container that emitted nothing still needs its marker.
        if !prefix.used {
            self.prefixes.push(prefix);
            self.line("");
            self.prefixes.pop();
        }
        result
    }

    /// Removes the prefixes of the open containers from a source line.
    fn strip(&self, line: &str) -> String {
        let mut rest = line.to_string();
        for prefix in &self.prefixes {
            if prefix.rest.trim_end() == ">" {
                rest = strip_quote(&rest).unwrap_or(&rest).to_string();
            } else {
                rest = strip_indent(&rest, prefix.rest.chars().count());
            }
        }
        rest
    }

    /// Source lines a node covers, with the container prefixes removed.
    fn source_lines(&self, node: &Node<'_>) -> Vec<String> {
        let first = self.index.line_of(node.range.start);
        let last = self.index.line_of(node.range.end.saturating_sub(1));
        (first..=last)
            .map(|n| {
                let range = self.index.line_range(self.src, n);
                self.strip(&self.src[range])
            })
            .collect()
    }

    /// Lines of a container that no child block covers, grouped into runs.
    /// Link reference definitions live here: the parser emits no event for them.
    fn uncovered(&self, range: &Range<usize>, children: &[Node<'_>]) -> Vec<(usize, Vec<String>)> {
        if range.is_empty() {
            return Vec::new();
        }
        let first = self.index.line_of(range.start);
        let last = self.index.line_of(range.end.saturating_sub(1));
        let mut covered = vec![false; last + 1 - first];
        for child in children {
            if child.range.is_empty() {
                continue;
            }
            let from = self.index.line_of(child.range.start).max(first);
            let to = self
                .index
                .line_of(child.range.end.saturating_sub(1))
                .min(last);
            for line in from..=to {
                covered[line - first] = true;
            }
        }
        let mut runs: Vec<(usize, Vec<String>)> = Vec::new();
        let mut open: Option<(usize, Vec<String>)> = None;
        for line in first..=last {
            let text = self.strip(&self.src[self.index.line_range(self.src, line)]);
            if covered[line - first] || text.trim().is_empty() {
                if let Some(run) = open.take() {
                    runs.push(run);
                }
                continue;
            }
            match &mut open {
                Some((_, lines)) => lines.push(text),
                None => open = Some((line, vec![text])),
            }
        }
        if let Some(run) = open {
            runs.push(run);
        }
        runs
    }

    fn blocks(&mut self, nodes: &[Node<'_>], tight: bool) {
        self.blocks_in(None, nodes, tight);
    }

    /// Emits the children of a container, keeping any line they do not cover.
    fn blocks_in(&mut self, range: Option<&Range<usize>>, nodes: &[Node<'_>], tight: bool) {
        // A task marker is a leaf of the item, written by the list emitter.
        let blocks: Vec<&Node<'_>> = nodes
            .iter()
            .filter(|n| !matches!(&n.kind, Kind::Leaf(Event::TaskListMarker(_))))
            .collect();
        let mut extra = match range {
            Some(range) => self.uncovered(range, nodes),
            None => Vec::new(),
        };
        extra.reverse();
        let mut previous_bullet: Option<char> = None;
        let mut emitted = 0;
        let mut emit_extra_before =
            |e: &mut Self, offset: usize, emitted: &mut usize, tight: bool| {
                while extra
                    .last()
                    .is_some_and(|(line, _)| e.index.line_of(offset) > *line)
                {
                    let (_, lines) = extra.pop().expect("checked above");
                    if *emitted > 0 && !tight {
                        e.blank_pending = true;
                    }
                    e.lines(&lines);
                    *emitted += 1;
                }
            };
        for node in &blocks {
            emit_extra_before(self, node.range.start, &mut emitted, tight);
            if emitted > 0 && !tight {
                self.blank_pending = true;
            }
            previous_bullet = self.block(node, previous_bullet);
            emitted += 1;
        }
        for (_, lines) in extra.into_iter().rev() {
            if emitted > 0 && !tight {
                self.blank_pending = true;
            }
            self.lines(&lines);
            emitted += 1;
        }
    }

    /// Writes one block; returns the bullet it used when it was a bullet list.
    fn block(&mut self, node: &Node<'_>, previous_bullet: Option<char>) -> Option<char> {
        match &node.kind {
            Kind::Paragraph { .. } => {
                self.paragraph(node);
                None
            }
            Kind::Heading { level } => {
                self.heading(node, *level);
                None
            }
            Kind::BlockQuote => {
                let range = node.range.clone();
                self.with_prefix(Prefix::same("> ".to_string()), |e| {
                    e.blocks_in(Some(&range), &node.children, false)
                });
                None
            }
            Kind::List { start } => self.list(node, *start, previous_bullet),
            Kind::CodeBlock(kind) => {
                self.code_block(node, kind);
                None
            }
            Kind::HtmlBlock | Kind::MetadataBlock => {
                let lines = self.source_lines(node);
                self.lines(&lines);
                None
            }
            Kind::Rule => {
                self.line(&self.opts.rule.clone());
                None
            }
            Kind::Table(align) => {
                self.table(node, align);
                None
            }
            Kind::FootnoteDef(label) => {
                let marker = format!("[^{label}]: ");
                let prefix = Prefix {
                    rest: " ".repeat(FOOTNOTE_INDENT),
                    first: marker,
                    used: false,
                };
                self.with_prefix(prefix, |e| e.blocks(&node.children, false));
                None
            }
            // Reference definitions and anything else are kept as written.
            _ => {
                let lines = self.source_lines(node);
                self.lines(&lines);
                None
            }
        }
    }

    fn atoms_of(&self, node: &Node<'_>) -> Vec<crate::cmd::fmt::inline::Atom> {
        let strip = |line: &str| self.strip(line);
        hazards::merge(tile(self.src, node, &strip))
    }

    fn paragraph(&mut self, node: &Node<'_>) {
        let atoms = self.atoms_of(node);
        let lines = wrap(&atoms, self.width());
        self.lines(&lines);
    }

    fn heading(&mut self, node: &Node<'_>, level: u8) {
        let atoms = self.atoms_of(node);
        let text = wrap(&atoms, UNLIMITED).join(" ");
        let hashes = "#".repeat(usize::from(level));
        if text.is_empty() {
            self.line(&hashes);
        } else {
            self.line(&format!("{hashes} {text}"));
        }
    }

    fn code_block(&mut self, node: &Node<'_>, kind: &CodeBlockKind<'_>) {
        match kind {
            // A fenced block already carries its fences in the source.
            CodeBlockKind::Fenced(_) => {
                let lines = self.source_lines(node);
                self.lines(&lines);
            }
            CodeBlockKind::Indented => {
                let lines = self.source_lines(node);
                self.lines(&lines);
            }
        }
    }

    fn list(
        &mut self,
        node: &Node<'_>,
        start: Option<u64>,
        previous_bullet: Option<char>,
    ) -> Option<char> {
        let tight = node.is_tight();
        let bullet = match previous_bullet {
            // Two bullet lists in a row would merge into one unless the marker changes.
            Some(previous) if start.is_none() => next_bullet(previous, self.opts.bullet),
            _ => self.opts.bullet,
        };
        let count = node.children.len() as u64;
        let last = start.map(|s| s + count.saturating_sub(1));
        let number_width = last.map(|n| n.to_string().len()).unwrap_or(0);
        for (i, item) in node.children.iter().enumerate() {
            if i > 0 && !tight {
                self.blank_pending = true;
            }
            let marker = match start {
                Some(s) => format!("{:>number_width$}{} ", s + i as u64, self.delimiter(node)),
                None => format!("{bullet} "),
            };
            let mut first = marker.clone();
            if let Some(done) = task_marker(item) {
                first.push_str(if done { "[x] " } else { "[ ] " });
            }
            let prefix = Prefix {
                rest: " ".repeat(first.chars().count()),
                first,
                used: false,
            };
            let range = item.range.clone();
            self.with_prefix(prefix, |e| e.blocks_in(Some(&range), &item.children, tight));
        }
        // Only a bullet list can merge with the next one.
        if start.is_some() { None } else { Some(bullet) }
    }

    /// Delimiter an ordered list uses in the source, `.` or `)`.
    fn delimiter(&self, list: &Node<'_>) -> char {
        let Some(item) = list.children.first() else {
            return '.';
        };
        let text = &self.src[item.range.clone()];
        let digits = text
            .trim_start()
            .trim_start_matches(|c: char| c.is_ascii_digit());
        if digits.starts_with(')') { ')' } else { '.' }
    }

    fn table(&mut self, node: &Node<'_>, align: &[Alignment]) {
        let lines = table::render(self.src, node, align, self.opts);
        self.lines(&lines);
    }
}

/// Bullet to use next so two adjacent lists stay separate.
fn next_bullet(previous: char, preferred: char) -> char {
    if previous != preferred {
        return preferred;
    }
    let at = BULLET_CYCLE
        .iter()
        .position(|c| *c == previous)
        .unwrap_or(0);
    BULLET_CYCLE[(at + 1) % BULLET_CYCLE.len()]
}

/// `Some(done)` when the item opens with a task marker.
fn task_marker(item: &Node<'_>) -> Option<bool> {
    fn first_leaf<'a, 'b>(node: &'a Node<'b>) -> Option<&'a Event<'b>> {
        match &node.kind {
            Kind::Leaf(event) => Some(event),
            _ => node.children.iter().find_map(first_leaf),
        }
    }
    match first_leaf(item) {
        Some(Event::TaskListMarker(done)) => Some(*done),
        _ => None,
    }
}
