//! The only module that emits escape sequences.

use std::io::{self, Write};
use std::rc::Rc;

use crate::model::Segment;
use crate::style::{ColorLevel, Style};

const ESC: &str = "\x1b";

pub struct AnsiWriter<W: Write> {
    out: W,
    level: ColorLevel,
    osc8: bool,
    current: Style,
    link: Option<Rc<str>>,
}

impl<W: Write> AnsiWriter<W> {
    /// `osc8` enables hyperlink sequences; it is ignored when `level` is `ColorLevel::None`.
    pub fn new(out: W, level: ColorLevel, osc8: bool) -> AnsiWriter<W> {
        AnsiWriter {
            out,
            level,
            osc8: osc8 && level != ColorLevel::None,
            current: Style::PLAIN,
            link: None,
        }
    }

    fn sgr(&self, style: Style) -> String {
        if style == Style::PLAIN {
            return format!("{ESC}[0m");
        }
        let mut s = format!("{ESC}[0");
        if style.bold {
            s.push_str(";1");
        }
        if style.dim {
            s.push_str(";2");
        }
        if style.italic {
            s.push_str(";3");
        }
        if style.underline {
            s.push_str(";4");
        }
        if style.strike {
            s.push_str(";9");
        }
        if let Some(c) = style.fg {
            c.push_sgr(self.level, true, &mut s);
        }
        if let Some(c) = style.bg {
            c.push_sgr(self.level, false, &mut s);
        }
        s.push('m');
        s
    }

    fn set_link(&mut self, link: &Option<Rc<str>>) -> io::Result<()> {
        if !self.osc8 || self.link == *link {
            return Ok(());
        }
        if self.link.is_some() {
            write!(self.out, "{ESC}]8;;{ESC}\\")?;
        }
        if let Some(url) = link {
            write!(self.out, "{ESC}]8;;{url}{ESC}\\")?;
        }
        self.link = link.clone();
        Ok(())
    }

    pub fn write_segment(&mut self, seg: &Segment) -> io::Result<()> {
        if seg.text.is_empty() {
            return Ok(());
        }
        if self.level != ColorLevel::None && seg.style != self.current {
            let sgr = self.sgr(seg.style);
            self.out.write_all(sgr.as_bytes())?;
            self.current = seg.style;
        }
        self.set_link(&seg.link)?;
        self.out.write_all(seg.text.as_bytes())
    }

    pub fn write_segments(&mut self, segs: &[Segment]) -> io::Result<()> {
        segs.iter().try_for_each(|s| self.write_segment(s))
    }

    /// Closes any open link and style, then writes the newline.
    pub fn end_line(&mut self) -> io::Result<()> {
        self.set_link(&None)?;
        if self.current != Style::PLAIN {
            let sgr = self.sgr(Style::PLAIN);
            self.out.write_all(sgr.as_bytes())?;
            self.current = Style::PLAIN;
        }
        self.out.write_all(b"\n")
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    fn run(level: ColorLevel, osc8: bool, segs: &[Segment]) -> String {
        let mut buf = Vec::new();
        {
            let mut w = AnsiWriter::new(&mut buf, level, osc8);
            w.write_segments(segs).unwrap();
            w.end_line().unwrap();
        }
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn no_escapes_at_level_none() {
        let segs = vec![
            Segment::new("a", Style::PLAIN.bold()),
            Segment {
                text: "b".into(),
                style: Style::PLAIN,
                link: Some(Rc::from("http://x")),
            },
        ];
        assert_eq!(run(ColorLevel::None, true, &segs), "ab\n");
    }

    #[test]
    fn one_sgr_per_change_and_reset_before_newline() {
        let bold = Style::PLAIN.bold();
        let segs = vec![
            Segment::new("a", bold),
            Segment::new("b", bold),
            Segment::new("c", Style::PLAIN.fg(Color::Ansi16(1))),
        ];
        assert_eq!(
            run(ColorLevel::Ansi16, false, &segs),
            "\x1b[0;1mab\x1b[0;31mc\x1b[0m\n"
        );
    }

    #[test]
    fn osc8_pairs() {
        let url: Rc<str> = Rc::from("http://x");
        let segs = vec![
            Segment {
                text: "link".into(),
                style: Style::PLAIN,
                link: Some(url.clone()),
            },
            Segment::new(" tail", Style::PLAIN),
        ];
        assert_eq!(
            run(ColorLevel::TrueColor, true, &segs),
            "\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\ tail\n"
        );
    }

    #[test]
    fn plain_style_line_has_no_reset() {
        let segs = vec![Segment::new("x", Style::PLAIN)];
        assert_eq!(run(ColorLevel::TrueColor, false, &segs), "x\n");
    }
}
