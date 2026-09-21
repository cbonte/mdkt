//! Text styling primitives shared by the parser, the highlighter and the ANSI writer.

/// A terminal colour. `Ansi16` and `Ansi256` hold palette indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Ansi16(u8),
    Ansi256(u8),
    Rgb(u8, u8, u8),
}

/// What the output terminal can display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorLevel {
    None,
    Ansi16,
    Ansi256,
    TrueColor,
}

/// Rendering attributes of one text segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub dim: bool,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

impl Style {
    pub const PLAIN: Style = Style {
        bold: false,
        italic: false,
        underline: false,
        strike: false,
        dim: false,
        fg: None,
        bg: None,
    };

    pub const fn bold(mut self) -> Style {
        self.bold = true;
        self
    }

    pub const fn italic(mut self) -> Style {
        self.italic = true;
        self
    }

    pub const fn underline(mut self) -> Style {
        self.underline = true;
        self
    }

    pub const fn strike(mut self) -> Style {
        self.strike = true;
        self
    }

    pub const fn dim(mut self) -> Style {
        self.dim = true;
        self
    }

    pub const fn fg(mut self, c: Color) -> Style {
        self.fg = Some(c);
        self
    }

    pub const fn bg(mut self, c: Color) -> Style {
        self.bg = Some(c);
        self
    }

    /// Layers `over` on top of `self`: flags are OR-ed, colours of `over` win when set.
    pub fn merge(self, over: Style) -> Style {
        Style {
            bold: self.bold || over.bold,
            italic: self.italic || over.italic,
            underline: self.underline || over.underline,
            strike: self.strike || over.strike,
            dim: self.dim || over.dim,
            fg: over.fg.or(self.fg),
            bg: over.bg.or(self.bg),
        }
    }
}

/// The 6 levels of the xterm 256-colour cube.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
/// Approximate sRGB values of the 16 base colours, in palette order.
const BASE16_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (170, 0, 0),
    (0, 170, 0),
    (170, 85, 0),
    (0, 0, 170),
    (170, 0, 170),
    (0, 170, 170),
    (170, 170, 170),
    (85, 85, 85),
    (255, 85, 85),
    (85, 255, 85),
    (255, 255, 85),
    (85, 85, 255),
    (255, 85, 255),
    (85, 255, 255),
    (255, 255, 255),
];

fn nearest_level(v: u8) -> usize {
    let mut best = 0;
    let mut best_d = u16::MAX;
    for (i, l) in CUBE_LEVELS.iter().enumerate() {
        let d = (v as i16 - *l as i16).unsigned_abs();
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

fn dist2(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let d = |x: u8, y: u8| {
        let d = x as i32 - y as i32;
        (d * d) as u32
    };
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

impl Color {
    /// sRGB approximation of the colour, used when downsampling.
    pub fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Ansi16(i) => BASE16_RGB[(i as usize) % 16],
            Color::Ansi256(i) => match i {
                0..=15 => BASE16_RGB[i as usize],
                16..=231 => {
                    let n = i as usize - 16;
                    (
                        CUBE_LEVELS[n / 36],
                        CUBE_LEVELS[(n / 6) % 6],
                        CUBE_LEVELS[n % 6],
                    )
                }
                _ => {
                    let g = 8 + 10 * (i as u16 - 232);
                    (g as u8, g as u8, g as u8)
                }
            },
        }
    }

    /// Nearest palette entry of the 256-colour set.
    pub fn to_ansi256(self) -> u8 {
        match self {
            Color::Ansi256(i) => i,
            Color::Ansi16(i) => i % 16,
            Color::Rgb(r, g, b) => {
                let cube = 16 + 36 * nearest_level(r) + 6 * nearest_level(g) + nearest_level(b);
                let cube = cube as u8;
                let avg = (r as u16 + g as u16 + b as u16) / 3;
                let grey_idx = ((avg as i32 - 8).max(0) / 10).min(23) as u8;
                let grey = 232 + grey_idx;
                let target = (r, g, b);
                if dist2(target, Color::Ansi256(grey).to_rgb())
                    < dist2(target, Color::Ansi256(cube).to_rgb())
                {
                    grey
                } else {
                    cube
                }
            }
        }
    }

    /// Nearest of the 16 base colours.
    pub fn to_ansi16(self) -> u8 {
        match self {
            Color::Ansi16(i) => i % 16,
            other => {
                let rgb = other.to_rgb();
                let mut best = 0u8;
                let mut best_d = u32::MAX;
                for (i, c) in BASE16_RGB.iter().enumerate() {
                    let d = dist2(rgb, *c);
                    if d < best_d {
                        best_d = d;
                        best = i as u8;
                    }
                }
                best
            }
        }
    }

    /// Appends the SGR parameters selecting this colour at `level`, or nothing at `ColorLevel::None`.
    pub fn push_sgr(self, level: ColorLevel, foreground: bool, out: &mut String) {
        use std::fmt::Write;
        match level {
            ColorLevel::None => {}
            ColorLevel::Ansi16 => {
                let i = self.to_ansi16();
                let base = match (foreground, i >= 8) {
                    (true, false) => 30,
                    (true, true) => 90,
                    (false, false) => 40,
                    (false, true) => 100,
                };
                let _ = write!(out, ";{}", base + (i % 8) as u16);
            }
            ColorLevel::Ansi256 => {
                let _ = write!(
                    out,
                    ";{};5;{}",
                    if foreground { 38 } else { 48 },
                    self.to_ansi256()
                );
            }
            ColorLevel::TrueColor => {
                let (r, g, b) = self.to_rgb();
                let _ = write!(
                    out,
                    ";{};2;{};{};{}",
                    if foreground { 38 } else { 48 },
                    r,
                    g,
                    b
                );
            }
        }
    }
}

/// Named style slots for every Markdown construct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    pub base: Style,
    pub heading: [Style; 6],
    pub heading_marker: Style,
    pub emphasis: Style,
    pub strong: Style,
    pub strike: Style,
    pub code_inline: Style,
    pub code_block: Style,
    pub quote_marker: Style,
    pub list_marker: Style,
    pub link: Style,
    pub link_url: Style,
    pub image: Style,
    pub rule: Style,
    pub table_border: Style,
    pub table_header: Style,
    pub task_done: Style,
    pub html: Style,
    /// File name printed above each document when several are rendered.
    pub file_header: Style,
}

impl Default for Theme {
    fn default() -> Theme {
        let plain = Style::PLAIN;
        let accent = Color::Ansi16(12);
        let heading = plain.bold().fg(accent);
        Theme {
            base: plain,
            heading: [
                heading.underline(),
                heading,
                heading,
                plain.bold(),
                plain.bold().italic(),
                plain.italic(),
            ],
            heading_marker: plain.dim(),
            emphasis: plain.italic(),
            strong: plain.bold(),
            strike: plain.strike(),
            code_inline: plain.fg(Color::Ansi16(11)),
            code_block: plain,
            quote_marker: plain.fg(Color::Ansi16(8)),
            list_marker: plain.fg(accent),
            link: plain.underline().fg(Color::Ansi16(14)),
            link_url: plain.dim(),
            image: plain.italic().fg(Color::Ansi16(13)),
            rule: plain.fg(Color::Ansi16(8)),
            table_border: plain.fg(Color::Ansi16(8)),
            table_header: plain.bold(),
            task_done: plain.fg(Color::Ansi16(10)),
            html: plain.dim(),
            file_header: plain.bold().fg(Color::Ansi16(13)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_primaries_downsample_to_base16() {
        assert_eq!(Color::Rgb(255, 0, 0).to_ansi16(), 1);
        assert_eq!(Color::Rgb(255, 120, 120).to_ansi16(), 9);
        assert_eq!(Color::Rgb(0, 0, 0).to_ansi16(), 0);
        assert_eq!(Color::Rgb(255, 255, 255).to_ansi16(), 15);
    }

    #[test]
    fn rgb_greys_pick_grey_ramp() {
        let c = Color::Rgb(128, 128, 128).to_ansi256();
        assert!((232..=255).contains(&c) || c == 8, "got {c}");
        assert_eq!(Color::Rgb(255, 0, 0).to_ansi256(), 196);
    }

    #[test]
    fn sgr_by_level() {
        let mut s = String::new();
        Color::Rgb(1, 2, 3).push_sgr(ColorLevel::TrueColor, true, &mut s);
        assert_eq!(s, ";38;2;1;2;3");
        s.clear();
        Color::Ansi16(9).push_sgr(ColorLevel::Ansi16, false, &mut s);
        assert_eq!(s, ";101");
        s.clear();
        Color::Ansi16(1).push_sgr(ColorLevel::None, true, &mut s);
        assert!(s.is_empty());
    }

    #[test]
    fn merge_layers_flags_and_colors() {
        let a = Style::PLAIN.bold().fg(Color::Ansi16(1));
        let b = Style::PLAIN.italic().fg(Color::Ansi16(2));
        let m = a.merge(b);
        assert!(m.bold && m.italic);
        assert_eq!(m.fg, Some(Color::Ansi16(2)));
        assert_eq!(a.merge(Style::PLAIN).fg, Some(Color::Ansi16(1)));
    }
}
