//! Width measurement and word wrapping over styled atoms.

use unicode_width::UnicodeWidthStr;

use crate::model::{Atom, InlineText, Line, Segment};
use crate::style::Style;

/// Rendering of a single newline inside a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum SoftBreaks {
    /// Join lines into one paragraph and reflow (CommonMark).
    #[default]
    Join,
    /// Keep each source line on its own output line.
    Preserve,
}

#[derive(Clone, Copy, Debug)]
pub struct WrapOptions {
    pub width: usize,
    pub soft_breaks: SoftBreaks,
}

/// Terminal columns occupied by `s`.
pub fn display_width(s: &str) -> usize {
    s.width()
}

pub fn segments_width(segs: &[Segment]) -> usize {
    segs.iter().map(|s| display_width(&s.text)).sum()
}

struct Wrapper {
    width: usize,
    lines: Vec<Line>,
    cur: Line,
    cur_w: usize,
    pending_space: bool,
}

impl Wrapper {
    fn flush(&mut self) {
        let line = std::mem::take(&mut self.cur);
        self.lines.push(line);
        self.cur_w = 0;
        self.pending_space = false;
    }

    fn push_word(&mut self, segs: &[Segment]) {
        let w = segments_width(segs);
        let sep = usize::from(self.pending_space && !self.cur.is_empty());
        if !self.cur.is_empty() && self.cur_w + sep + w > self.width {
            self.flush();
        }
        if self.pending_space && !self.cur.is_empty() {
            // The space carries the surrounding style only when both neighbours share it.
            let last = self.cur.last().expect("non-empty line");
            let space = match segs.first() {
                Some(next) if next.style == last.style && next.link == last.link => Segment {
                    text: " ".into(),
                    style: last.style,
                    link: last.link.clone(),
                },
                _ => Segment::new(" ", Style::PLAIN),
            };
            self.cur.push(space);
            self.cur_w += 1;
        }
        self.pending_space = false;
        if w <= self.width {
            self.cur.extend_from_slice(segs);
            self.cur_w += w;
            return;
        }
        self.push_chunks(segs);
    }

    /// Splits a word wider than the line into chunks that each fit on a line.
    fn push_chunks(&mut self, segs: &[Segment]) {
        for seg in segs {
            for ch in seg.text.chars() {
                let cw = display_width(ch.encode_utf8(&mut [0; 4]));
                if self.cur_w + cw > self.width && !self.cur.is_empty() {
                    self.flush();
                }
                match self.cur.last_mut() {
                    Some(last) if last.style == seg.style && last.link == seg.link => {
                        last.text.push(ch)
                    }
                    _ => self.cur.push(Segment {
                        text: ch.to_string(),
                        style: seg.style,
                        link: seg.link.clone(),
                    }),
                }
                self.cur_w += cw;
            }
        }
    }
}

/// Wraps `text` to `opts.width` columns. Lines carry no trailing spaces.
pub fn wrap(text: &InlineText, opts: &WrapOptions) -> Vec<Line> {
    let mut w = Wrapper {
        width: opts.width.max(1),
        lines: Vec::new(),
        cur: Vec::new(),
        cur_w: 0,
        pending_space: false,
    };
    for atom in &text.0 {
        match atom {
            Atom::Word(segs) => w.push_word(segs),
            Atom::Space => w.pending_space = true,
            Atom::SoftBreak => match opts.soft_breaks {
                SoftBreaks::Join => w.pending_space = true,
                SoftBreaks::Preserve => w.flush(),
            },
            Atom::HardBreak => w.flush(),
        }
    }
    if !w.cur.is_empty() {
        w.flush();
    }
    w.lines
}

/// Cuts `line` to `width` columns, ending with `ellipsis` when something was removed.
pub fn truncate_line(line: &Line, width: usize, ellipsis: &str) -> Line {
    if segments_width(line) <= width {
        return line.clone();
    }
    let ell_w = display_width(ellipsis);
    let budget = width.saturating_sub(ell_w);
    let mut out: Line = Vec::new();
    let mut used = 0;
    'outer: for seg in line {
        let mut text = String::new();
        for ch in seg.text.chars() {
            let cw = display_width(ch.encode_utf8(&mut [0; 4]));
            if used + cw > budget {
                if !text.is_empty() {
                    out.push(Segment {
                        text,
                        style: seg.style,
                        link: seg.link.clone(),
                    });
                }
                break 'outer;
            }
            used += cw;
            text.push(ch);
        }
        out.push(Segment {
            text,
            style: seg.style,
            link: seg.link.clone(),
        });
    }
    if ell_w > 0 && ell_w <= width {
        let style = out.last().map(|s| s.style).unwrap_or_default();
        out.push(Segment::new(ellipsis, style));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(s: &str) -> Atom {
        Atom::Word(vec![Segment::new(s, Style::PLAIN)])
    }

    fn text(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect()
    }

    fn opts(width: usize, soft_breaks: SoftBreaks) -> WrapOptions {
        WrapOptions { width, soft_breaks }
    }

    #[test]
    fn widths() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("\u{4e2d}\u{6587}"), 4);
        assert_eq!(display_width("e\u{301}"), 1);
    }

    #[test]
    fn greedy_wrap_at_boundary() {
        let t = InlineText(vec![
            word("aaa"),
            Atom::Space,
            word("bbb"),
            Atom::Space,
            word("cc"),
        ]);
        assert_eq!(
            text(&wrap(&t, &opts(7, SoftBreaks::Join))),
            ["aaa bbb", "cc"]
        );
        assert_eq!(
            text(&wrap(&t, &opts(6, SoftBreaks::Join))),
            ["aaa", "bbb cc"]
        );
    }

    #[test]
    fn no_trailing_space() {
        let t = InlineText(vec![word("aaa"), Atom::Space, word("bbbb"), Atom::Space]);
        assert_eq!(text(&wrap(&t, &opts(4, SoftBreaks::Join))), ["aaa", "bbbb"]);
    }

    #[test]
    fn overlong_word_is_chunked() {
        let t = InlineText(vec![word("ab"), Atom::Space, word("abcdefghij")]);
        assert_eq!(
            text(&wrap(&t, &opts(4, SoftBreaks::Join))),
            ["ab", "abcd", "efgh", "ij"]
        );
        let t = InlineText(vec![word("x")]);
        assert_eq!(text(&wrap(&t, &opts(0, SoftBreaks::Join))), ["x"]);
    }

    #[test]
    fn mixed_style_word_stays_together() {
        let t = InlineText(vec![
            word("aaaa"),
            Atom::Space,
            Atom::Word(vec![
                Segment::new("bo", Style::PLAIN.bold()),
                Segment::new("ld", Style::PLAIN),
            ]),
        ]);
        let lines = wrap(&t, &opts(6, SoftBreaks::Join));
        assert_eq!(text(&lines), ["aaaa", "bold"]);
        assert_eq!(lines[1].len(), 2);
    }

    #[test]
    fn soft_break_modes() {
        let t = InlineText(vec![
            word("one"),
            Atom::SoftBreak,
            word("two"),
            Atom::SoftBreak,
            word("three"),
        ]);
        assert_eq!(
            text(&wrap(&t, &opts(40, SoftBreaks::Join))),
            ["one two three"]
        );
        assert_eq!(
            text(&wrap(&t, &opts(40, SoftBreaks::Preserve))),
            ["one", "two", "three"]
        );
    }

    #[test]
    fn preserve_still_wraps_long_source_lines() {
        let t = InlineText(vec![
            word("aaa"),
            Atom::Space,
            word("bbb"),
            Atom::SoftBreak,
            word("c"),
        ]);
        assert_eq!(
            text(&wrap(&t, &opts(5, SoftBreaks::Preserve))),
            ["aaa", "bbb", "c"]
        );
    }

    #[test]
    fn hard_break_doubled_gives_empty_line() {
        let t = InlineText(vec![word("a"), Atom::HardBreak, Atom::HardBreak, word("b")]);
        assert_eq!(text(&wrap(&t, &opts(10, SoftBreaks::Join))), ["a", "", "b"]);
    }

    #[test]
    fn truncate_adds_ellipsis() {
        let line = vec![Segment::new("abcdefgh", Style::PLAIN)];
        assert_eq!(text(&[truncate_line(&line, 5, "...")]), ["ab..."]);
        assert_eq!(text(&[truncate_line(&line, 8, "...")]), ["abcdefgh"]);
    }
}
