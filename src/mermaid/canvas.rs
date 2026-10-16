//! Character grid on which a diagram is drawn, and its conversion to styled lines.

use unicode_width::UnicodeWidthChar;

use crate::model::{Line, Segment};
use crate::options::Glyphs;
use crate::style::Style;
use crate::table::Borders;

/// Line segment bits of a cell.
pub const N: u8 = 1;
pub const E: u8 = 2;
pub const S: u8 = 4;
pub const W: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    N,
    E,
    S,
    W,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::N, Dir::E, Dir::S, Dir::W];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn bit(self) -> u8 {
        1 << self.index()
    }

    pub fn opposite(self) -> Dir {
        Dir::ALL[(self.index() + 2) % 4]
    }

    /// The cell next to `(x, y)` in this direction. The caller keeps a margin around the grid.
    pub fn step(self, (x, y): (usize, usize)) -> (usize, usize) {
        match self {
            Dir::N => (x, y - 1),
            Dir::E => (x + 1, y),
            Dir::S => (x, y + 1),
            Dir::W => (x - 1, y),
        }
    }

    /// Direction of the move from `a` to the adjacent cell `b`.
    pub fn between(a: (usize, usize), b: (usize, usize)) -> Dir {
        if b.1 < a.1 {
            Dir::N
        } else if b.0 > a.0 {
            Dir::E
        } else if b.1 > a.1 {
            Dir::S
        } else {
            Dir::W
        }
    }
}

/// How the lines of a cell are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Normal,
    Dotted,
    Thick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
    Empty,
    /// Line segments leaving the cell: a mask of `N`, `E`, `S` and `W`.
    Lines(u8, Weight),
    /// A drawing character that is not part of a line.
    Glyph(char),
    Text(char),
    /// Right half of a double-width `Text`.
    Cont,
}

pub struct Canvas {
    w: usize,
    h: usize,
    cells: Vec<Cell>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            cells: vec![Cell::Empty; w * h],
        }
    }

    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }

    /// Line bits of a cell, 0 when it holds none.
    pub fn lines(&self, x: usize, y: usize) -> u8 {
        match self.cells[y * self.w + x] {
            Cell::Lines(m, _) => m,
            _ => 0,
        }
    }

    /// True when a line may pass through the cell.
    pub fn is_free(&self, x: usize, y: usize) -> bool {
        matches!(self.cells[y * self.w + x], Cell::Empty | Cell::Lines(..))
    }

    /// True when nothing is drawn in the cell.
    pub fn is_empty(&self, x: usize, y: usize) -> bool {
        self.cells[y * self.w + x] == Cell::Empty
    }

    /// Adds line segments to a cell. A weight other than `Normal` replaces the cell's.
    pub fn add_lines(&mut self, x: usize, y: usize, mask: u8, weight: Weight) {
        let cell = &mut self.cells[y * self.w + x];
        match *cell {
            Cell::Empty => *cell = Cell::Lines(mask, weight),
            Cell::Lines(m, old) => {
                let weight = if weight == Weight::Normal {
                    old
                } else {
                    weight
                };
                *cell = Cell::Lines(m | mask, weight);
            }
            _ => {}
        }
    }

    pub fn set_glyph(&mut self, x: usize, y: usize, ch: char) {
        self.cells[y * self.w + x] = Cell::Glyph(ch);
    }

    /// Writes `s` from `(x, y)` rightwards and returns the columns used. Stops at the right edge.
    pub fn text(&mut self, x: usize, y: usize, s: &str) -> usize {
        let mut col = x;
        for ch in s.chars() {
            let width = ch.width().unwrap_or(0);
            if width == 0 || col + width > self.w {
                continue;
            }
            self.cells[y * self.w + col] = Cell::Text(ch);
            if width == 2 {
                self.cells[y * self.w + col + 1] = Cell::Cont;
            }
            col += width;
        }
        col - x
    }

    /// Draws a rectangle border of `w` by `h` cells, at least 2 by 2.
    pub fn draw_box(&mut self, x: usize, y: usize, w: usize, h: usize) {
        for cx in x + 1..x + w - 1 {
            self.add_lines(cx, y, E | W, Weight::Normal);
            self.add_lines(cx, y + h - 1, E | W, Weight::Normal);
        }
        for cy in y + 1..y + h - 1 {
            self.add_lines(x, cy, N | S, Weight::Normal);
            self.add_lines(x + w - 1, cy, N | S, Weight::Normal);
        }
        self.add_lines(x, y, S | E, Weight::Normal);
        self.add_lines(x + w - 1, y, S | W, Weight::Normal);
        self.add_lines(x, y + h - 1, N | E, Weight::Normal);
        self.add_lines(x + w - 1, y + h - 1, N | W, Weight::Normal);
    }

    /// One line per row, without blank rows at the ends nor blank columns on the left.
    pub fn into_lines(self, glyphs: &Glyphs, line: Style, text: Style) -> Vec<Line> {
        let rows: Vec<&[Cell]> = self.cells.chunks(self.w.max(1)).collect();
        let used = |row: &&[Cell]| row.iter().any(|c| *c != Cell::Empty);
        let Some(first) = rows.iter().position(used) else {
            return Vec::new();
        };
        let last = rows.iter().rposition(used).unwrap_or(first);
        let left = rows[first..=last]
            .iter()
            .filter_map(|r| r.iter().position(|c| *c != Cell::Empty))
            .min()
            .unwrap_or(0);
        rows[first..=last]
            .iter()
            .map(|row| {
                let end = row
                    .iter()
                    .rposition(|c| *c != Cell::Empty)
                    .map_or(0, |i| i + 1);
                let mut segs: Vec<Segment> = Vec::new();
                let mut push = |ch: char, style: Style| match segs.last_mut() {
                    Some(seg) if seg.style == style => seg.text.push(ch),
                    _ => segs.push(Segment::new(ch.to_string(), style)),
                };
                for cell in &row[left.min(end)..end] {
                    match *cell {
                        Cell::Empty => push(' ', text),
                        Cell::Lines(m, weight) => push(lines_glyph(m, weight, glyphs), line),
                        Cell::Glyph(ch) => push(ch, line),
                        Cell::Text(ch) => push(ch, text),
                        Cell::Cont => {}
                    }
                }
                segs
            })
            .collect()
    }
}

/// The character joining the line segments of `mask`.
fn lines_glyph(mask: u8, weight: Weight, glyphs: &Glyphs) -> char {
    let b = if weight == Weight::Thick {
        &glyphs.thick
    } else {
        &glyphs.borders
    };
    let joined = joined_glyph(mask, b);
    match weight {
        Weight::Dotted if mask & (E | W) == 0 => glyphs.dotted[1],
        Weight::Dotted if mask & (N | S) == 0 => glyphs.dotted[0],
        _ => joined,
    }
}

fn joined_glyph(mask: u8, b: &Borders) -> char {
    match (mask & N != 0, mask & E != 0, mask & S != 0, mask & W != 0) {
        (_, false, _, false) => b.v,
        (false, _, false, _) => b.h,
        (false, true, true, false) => b.top_left,
        (false, false, true, true) => b.top_right,
        (true, true, false, false) => b.bottom_left,
        (true, false, false, true) => b.bottom_right,
        (true, true, true, false) => b.mid_left,
        (true, false, true, true) => b.mid_right,
        (false, true, true, true) => b.top_mid,
        (true, true, false, true) => b.bottom_mid,
        (true, true, true, true) => b.cross,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(canvas: Canvas) -> Vec<String> {
        canvas
            .into_lines(&Glyphs::ascii(), Style::PLAIN, Style::PLAIN)
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn box_with_text() {
        let mut c = Canvas::new(10, 6);
        c.draw_box(2, 1, 7, 3);
        c.text(4, 2, "ab");
        assert_eq!(plain(c), ["+-----+", "| ab  |", "+-----+"]);
    }

    #[test]
    fn lines_join_into_a_junction() {
        let mut c = Canvas::new(5, 3);
        c.add_lines(1, 1, E | W, Weight::Normal);
        c.add_lines(2, 1, E | W | S, Weight::Normal);
        c.add_lines(2, 2, N, Weight::Normal);
        assert_eq!(plain(c), ["-+", " |"]);
    }

    #[test]
    fn wide_text_takes_two_cells() {
        let mut c = Canvas::new(6, 1);
        assert_eq!(c.text(0, 0, "a\u{4e2d}b"), 4);
        assert_eq!(plain(c), ["a\u{4e2d}b"]);
    }
}
