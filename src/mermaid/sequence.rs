//! Sequence diagrams: parsing, column layout and drawing.

use std::collections::HashMap;

use super::canvas::{Canvas, Dir, E, N, S, W, Weight};
use crate::model::Line;
use crate::options::RenderOptions;
use crate::wrap::{display_width, segments_width};

/// Border plus one blank cell on each side of a name or note text.
const BOX_PAD: usize = 4;
/// Rows of a participant box.
const HEADER_ROWS: usize = 3;
/// Rows of a message: its label, its line and a blank row.
const MESSAGE_ROWS: usize = 3;
/// Blank cells kept between two participant boxes.
const BOX_GAP: usize = 2;
/// Cells a message line needs on top of its label.
const LABEL_ROOM: usize = 4;

#[derive(Debug, PartialEq, Eq)]
enum Place {
    Left(usize),
    Right(usize),
    Over(usize, usize),
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Message {
        from: usize,
        to: usize,
        text: String,
        dashed: bool,
        arrow: bool,
    },
    Note {
        place: Place,
        lines: Vec<String>,
    },
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Sequence {
    names: Vec<String>,
    events: Vec<Event>,
}

/// Message operators with their line style and arrow head.
const OPERATORS: [(&str, bool, bool); 6] = [
    ("-->>", true, true),
    ("->>", false, true),
    ("-->", true, false),
    ("->", false, false),
    ("--)", true, true),
    ("-)", false, true),
];

/// Keywords that open blocks or features that are not drawn.
const UNSUPPORTED: [&str; 17] = [
    "loop",
    "alt",
    "opt",
    "par",
    "critical",
    "break",
    "rect",
    "else",
    "and",
    "end",
    "box",
    "create",
    "destroy",
    "link",
    "links",
    "properties",
    "details",
];

struct Parser {
    seq: Sequence,
    ids: HashMap<String, usize>,
    numbered: bool,
    count: usize,
}

impl Parser {
    fn participant(&mut self, id: &str) -> usize {
        *self.ids.entry(id.to_string()).or_insert_with(|| {
            self.seq.names.push(id.to_string());
            self.seq.names.len() - 1
        })
    }

    fn declare(&mut self, rest: &str) {
        let (id, name) = match rest.split_once(" as ") {
            Some((id, name)) => (id.trim(), name.trim()),
            None => (rest.trim(), rest.trim()),
        };
        let index = self.participant(id);
        self.seq.names[index] = name.to_string();
    }

    fn note(&mut self, rest: &str) -> Option<()> {
        let (head, text) = rest.split_once(':')?;
        let head = head.trim();
        let place = if let Some(who) = head.strip_prefix("right of ") {
            Place::Right(self.participant(who.trim()))
        } else if let Some(who) = head.strip_prefix("left of ") {
            Place::Left(self.participant(who.trim()))
        } else {
            let who = head.strip_prefix("over ")?;
            match who.split_once(',') {
                Some((a, b)) => {
                    let (a, b) = (self.participant(a.trim()), self.participant(b.trim()));
                    Place::Over(a.min(b), a.max(b))
                }
                None => {
                    let a = self.participant(who.trim());
                    Place::Over(a, a)
                }
            }
        };
        let lines: Vec<String> = text
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("<br>", "\n")
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() {
            return None;
        }
        self.seq.events.push(Event::Note { place, lines });
        Some(())
    }

    fn message(&mut self, line: &str) -> Option<()> {
        let (head, text) = match line.split_once(':') {
            Some((head, text)) => (head, text.trim()),
            None => (line, ""),
        };
        let (at, op, dashed, arrow) = OPERATORS
            .iter()
            .filter_map(|&(op, dashed, arrow)| head.find(op).map(|at| (at, op, dashed, arrow)))
            .min_by_key(|&(at, op, ..)| (at, std::cmp::Reverse(op.len())))?;
        let from = head[..at].trim();
        let to = head[at + op.len()..].trim().trim_start_matches(['+', '-']);
        if from.is_empty() || to.is_empty() {
            return None;
        }
        let (from, to) = (self.participant(from), self.participant(to));
        if from == to {
            return None;
        }
        self.count += 1;
        let text = text
            .replace("<br/>", " ")
            .replace("<br />", " ")
            .replace("<br>", " ");
        let text = if self.numbered {
            format!("{}. {text}", self.count)
        } else {
            text
        };
        self.seq.events.push(Event::Message {
            from,
            to,
            text: text.trim().to_string(),
            dashed,
            arrow,
        });
        Some(())
    }
}

/// Parses a `sequenceDiagram` block. `None` for another diagram type and for syntax that is not
/// drawn.
fn parse(src: &str) -> Option<Sequence> {
    let mut lines = src
        .lines()
        .map(|l| l.trim().trim_end_matches(';').trim_end())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    if lines.next()? != "sequenceDiagram" {
        return None;
    }
    let mut parser = Parser {
        seq: Sequence::default(),
        ids: HashMap::new(),
        numbered: false,
        count: 0,
    };
    for line in lines {
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        match word {
            "participant" | "actor" => parser.declare(rest),
            "autonumber" => parser.numbered = true,
            "activate" | "deactivate" | "title" => {}
            "Note" | "note" => parser.note(rest)?,
            _ if UNSUPPORTED.contains(&word) => return None,
            _ => parser.message(line)?,
        }
    }
    (!parser.seq.names.is_empty()).then_some(parser.seq)
}

fn text_width(lines: &[String]) -> usize {
    lines.iter().map(|l| display_width(l)).max().unwrap_or(0)
}

/// Raises the distances between the centers `lo..hi` so that they add up to at least `need`.
fn widen(dist: &mut [usize], lo: usize, hi: usize, need: usize) {
    let have: usize = dist[lo..hi].iter().sum();
    if have < need {
        dist[hi - 1] += need - have;
    }
}

/// Box of a note: left column and width, with the participant centers `c`.
fn note_box(place: &Place, lines: &[String], c: &[i64]) -> (i64, usize) {
    let text = text_width(lines) + BOX_PAD;
    let width = |w: usize| i64::try_from(w).unwrap_or(i64::MAX);
    match *place {
        Place::Right(a) => (c[a] + 2, text),
        Place::Left(a) => (c[a] - 2 - width(text), text),
        Place::Over(a, b) => {
            let w = text.max(usize::try_from(c[b] - c[a]).unwrap_or(0) + 5);
            ((c[a] + c[b]) / 2 - width(w) / 2, w)
        }
    }
}

fn draw(seq: &Sequence, width: usize, opts: &RenderOptions) -> Option<Vec<Line>> {
    let n = seq.names.len();
    let boxes: Vec<usize> = seq
        .names
        .iter()
        .map(|s| display_width(s) + BOX_PAD)
        .collect();
    let mut dist: Vec<usize> = (1..n)
        .map(|i| (boxes[i - 1] + boxes[i]).div_ceil(2) + BOX_GAP)
        .collect();
    for event in &seq.events {
        match event {
            Event::Message { from, to, text, .. } => {
                let (lo, hi) = (*from.min(to), *from.max(to));
                widen(&mut dist, lo, hi, display_width(text) + LABEL_ROOM);
            }
            Event::Note { place, lines } => {
                let need = text_width(lines) + BOX_PAD + 3;
                match *place {
                    Place::Right(a) if a + 1 < n => widen(&mut dist, a, a + 1, need),
                    Place::Left(a) if a > 0 => widen(&mut dist, a - 1, a, need),
                    Place::Over(a, b) if a == b => {
                        let half = need / 2 + 1;
                        if a > 0 {
                            widen(&mut dist, a - 1, a, half);
                        }
                        if a + 1 < n {
                            widen(&mut dist, a, a + 1, half);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut centers = vec![0i64; n];
    for i in 1..n {
        centers[i] = centers[i - 1] + i64::try_from(dist[i - 1]).ok()?;
    }
    let half = |i: usize| i64::try_from(boxes[i] / 2).unwrap_or(0);
    let mut left = (0..n).map(|i| centers[i] - half(i)).min()?;
    let mut right = (0..n)
        .map(|i| centers[i] - half(i) + i64::try_from(boxes[i]).unwrap_or(0))
        .max()?;
    for event in &seq.events {
        if let Event::Note { place, lines } = event {
            let (x, w) = note_box(place, lines, &centers);
            left = left.min(x);
            right = right.max(x + i64::try_from(w).ok()?);
        }
    }
    let shift = |x: i64| usize::try_from(x - left).ok();
    let rows: usize = seq
        .events
        .iter()
        .map(|e| match e {
            Event::Message { .. } => MESSAGE_ROWS,
            Event::Note { lines, .. } => lines.len() + 3,
        })
        .sum();
    let end = HEADER_ROWS + 1 + rows;
    let mut canvas = Canvas::new(usize::try_from(right - left).ok()?, end + HEADER_ROWS);
    let columns: Vec<usize> = centers.iter().map(|&c| shift(c)).collect::<Option<_>>()?;
    for (i, name) in seq.names.iter().enumerate() {
        let x = columns[i] - boxes[i] / 2;
        for y in [0, end] {
            canvas.draw_box(x, y, boxes[i], HEADER_ROWS);
            canvas.text(x + BOX_PAD / 2, y + 1, name);
        }
        canvas.add_lines(columns[i], HEADER_ROWS - 1, S, Weight::Normal);
        for y in HEADER_ROWS..end {
            canvas.add_lines(columns[i], y, N | S, Weight::Dotted);
        }
        canvas.add_lines(columns[i], end, N, Weight::Normal);
    }
    let mut y = HEADER_ROWS + 1;
    for event in &seq.events {
        match event {
            Event::Message {
                from,
                to,
                text,
                dashed,
                arrow,
            } => {
                let weight = if *dashed {
                    Weight::Dotted
                } else {
                    Weight::Normal
                };
                let (a, b) = (columns[*from], columns[*to]);
                let (lo, hi) = (a.min(b), a.max(b));
                let label = display_width(text);
                canvas.text(lo + 1 + (hi - lo - 1 - label) / 2, y, text);
                let (dir, near) = if a < b { (Dir::E, a) } else { (Dir::W, a) };
                let far = b;
                canvas.add_lines(near, y + 1, dir.bit(), Weight::Normal);
                for x in lo + 1..hi {
                    canvas.add_lines(x, y + 1, E | W, weight);
                }
                let tip = if a < b { far - 1 } else { far + 1 };
                if *arrow {
                    canvas.set_glyph(tip, y + 1, opts.glyphs.arrows[dir.index()]);
                } else {
                    canvas.add_lines(far, y + 1, dir.opposite().bit(), Weight::Normal);
                }
                y += MESSAGE_ROWS;
            }
            Event::Note { place, lines } => {
                let (x, w) = note_box(place, lines, &centers);
                let (x, h) = (shift(x)?, lines.len() + 2);
                canvas.clear_rect(x, y, w, h);
                canvas.draw_box(x, y, w, h);
                for (i, line) in lines.iter().enumerate() {
                    canvas.text(x + BOX_PAD / 2, y + 1 + i, line);
                }
                y += h + 1;
            }
        }
    }
    let lines = canvas.into_lines(&opts.glyphs, opts.theme.table_border, opts.theme.base);
    if lines.iter().any(|l| segments_width(l) > width) {
        return None;
    }
    Some(lines)
}

/// Draws a `sequenceDiagram` block within `width` columns.
pub fn render(code: &str, width: usize, opts: &RenderOptions) -> Option<Vec<Line>> {
    draw(&parse(code)?, width, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw_plain(src: &str, width: usize) -> Option<String> {
        let lines = render(src, width, &RenderOptions::plain(width))?;
        Some(
            lines
                .iter()
                .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>() + "\n")
                .collect(),
        )
    }

    #[test]
    fn parses_participants_messages_and_notes() {
        let seq = parse(
            "sequenceDiagram\n participant A as Alice\n A->>B: hi\n B-->>A: ok\n Note over A,B: n\n",
        )
        .unwrap();
        assert_eq!(seq.names, ["Alice", "B"]);
        assert_eq!(seq.events.len(), 3);
        assert!(matches!(
            &seq.events[1],
            Event::Message {
                dashed: true,
                arrow: true,
                ..
            }
        ));
    }

    #[test]
    fn rejects_blocks_and_self_messages() {
        assert!(parse("sequenceDiagram\n loop x\n A->>B: y\n end").is_none());
        assert!(parse("sequenceDiagram\n A->>A: y").is_none());
        assert!(parse("graph TD\n A --> B").is_none());
        assert!(parse("sequenceDiagram\n A-xB: y").is_none());
    }

    #[test]
    fn autonumber_prefixes_labels() {
        let seq = parse("sequenceDiagram\n autonumber\n A->>B: hi\n").unwrap();
        assert!(matches!(&seq.events[0], Event::Message { text, .. } if text == "1. hi"));
    }

    #[test]
    fn two_participants() {
        let art = draw_plain("sequenceDiagram\n A->>B: hello\n B-->>A: ok\n", 40).unwrap();
        let expected = "\
+---+    +---+
| A |    | B |
+-+-+    +-+-+
  :        :
  : hello  :
  +------->:
  :        :
  :   ok   :
  :<.......+
  :        :
+-+-+    +-+-+
| A |    | B |
+---+    +---+
";
        assert_eq!(art, expected);
    }

    #[test]
    fn too_wide_falls_back() {
        assert!(draw_plain("sequenceDiagram\n A->>B: a long message here\n", 10).is_none());
    }
}
