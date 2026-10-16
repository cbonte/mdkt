//! GFM table layout: column measurement, width distribution and border drawing.

use crate::model::{Align, Atom, InlineText, Line, Segment, Table};
use crate::options::{RenderOptions, TableOverflow};
use crate::style::Style;
use crate::wrap::{SoftBreaks, WrapOptions, segments_width, truncate_line, wrap};

/// Characters used to draw a table frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Borders {
    pub h: char,
    pub v: char,
    pub top_left: char,
    pub top_mid: char,
    pub top_right: char,
    pub mid_left: char,
    pub cross: char,
    pub mid_right: char,
    pub bottom_left: char,
    pub bottom_mid: char,
    pub bottom_right: char,
}

pub const UNICODE_BORDERS: Borders = Borders {
    h: '\u{2500}',
    v: '\u{2502}',
    top_left: '\u{250c}',
    top_mid: '\u{252c}',
    top_right: '\u{2510}',
    mid_left: '\u{251c}',
    cross: '\u{253c}',
    mid_right: '\u{2524}',
    bottom_left: '\u{2514}',
    bottom_mid: '\u{2534}',
    bottom_right: '\u{2518}',
};

pub const ASCII_BORDERS: Borders = Borders {
    h: '-',
    v: '|',
    top_left: '+',
    top_mid: '+',
    top_right: '+',
    mid_left: '+',
    cross: '+',
    mid_right: '+',
    bottom_left: '+',
    bottom_mid: '+',
    bottom_right: '+',
};

pub const THICK_BORDERS: Borders = Borders {
    h: '\u{2501}',
    v: '\u{2503}',
    top_left: '\u{250f}',
    top_mid: '\u{2533}',
    top_right: '\u{2513}',
    mid_left: '\u{2523}',
    cross: '\u{254b}',
    mid_right: '\u{252b}',
    bottom_left: '\u{2517}',
    bottom_mid: '\u{253b}',
    bottom_right: '\u{251b}',
};

pub const ASCII_THICK_BORDERS: Borders = Borders {
    h: '=',
    ..ASCII_BORDERS
};

/// Widths of the longest single-line rendering and of the widest word of a cell.
fn measure_cell(cell: &InlineText) -> (usize, usize) {
    let natural = wrap(
        cell,
        &WrapOptions {
            width: usize::MAX,
            soft_breaks: SoftBreaks::Join,
        },
    )
    .iter()
    .map(|l| segments_width(l))
    .max()
    .unwrap_or(0);
    let min = cell
        .0
        .iter()
        .filter_map(|a| match a {
            Atom::Word(segs) => Some(segments_width(segs)),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    (natural, min)
}

/// Per column: (minimum width, natural width).
pub fn measure(table: &Table) -> (Vec<usize>, Vec<usize>) {
    let cols = table.columns();
    let mut min = vec![1; cols];
    let mut natural = vec![1; cols];
    for row in std::iter::once(&table.head).chain(table.rows.iter()) {
        for (i, cell) in row.iter().enumerate() {
            let (n, m) = measure_cell(cell);
            natural[i] = natural[i].max(n);
            min[i] = min[i].max(m);
        }
    }
    (min, natural)
}

/// Column widths fitting `available`, and whether the table cannot fit even at minimum widths.
pub fn distribute(min: &[usize], natural: &[usize], available: usize) -> (Vec<usize>, bool) {
    let n = min.len();
    if n == 0 {
        return (Vec::new(), false);
    }
    let min: Vec<usize> = min.iter().map(|m| (*m).max(1)).collect();
    let natural: Vec<usize> = natural
        .iter()
        .zip(&min)
        .map(|(n, m)| (*n).max(*m))
        .collect();
    let total_natural: usize = natural.iter().sum();
    if total_natural <= available {
        return (natural, false);
    }
    let total_min: usize = min.iter().sum();
    if total_min > available {
        let widths = min
            .iter()
            .map(|m| (m * available / total_min).max(1))
            .collect();
        return (widths, true);
    }
    let mut widths = min.clone();
    let mut extra = available - total_min;
    while extra > 0 {
        let growable: Vec<usize> = (0..n).filter(|&i| widths[i] < natural[i]).collect();
        if growable.is_empty() {
            break;
        }
        let share = (extra / growable.len()).max(1);
        for i in growable {
            let add = share.min(natural[i] - widths[i]).min(extra);
            widths[i] += add;
            extra -= add;
            if extra == 0 {
                break;
            }
        }
    }
    (widths, false)
}

fn pad(line: &mut Line, width: usize, align: Align, style: Style) {
    let w = segments_width(line);
    if w >= width {
        return;
    }
    let total = width - w;
    let (left, right) = match align {
        Align::Left => (0, total),
        Align::Right => (total, 0),
        Align::Center => (total / 2, total - total / 2),
    };
    if left > 0 {
        line.insert(0, Segment::new(" ".repeat(left), style));
    }
    if right > 0 {
        line.push(Segment::new(" ".repeat(right), style));
    }
}

fn cell_lines(cell: &InlineText, width: usize, opts: &RenderOptions) -> Vec<Line> {
    match opts.table_overflow {
        TableOverflow::Wrap => wrap(
            cell,
            &WrapOptions {
                width,
                soft_breaks: SoftBreaks::Join,
            },
        ),
        TableOverflow::Truncate => {
            let joined = wrap(
                cell,
                &WrapOptions {
                    width: usize::MAX,
                    soft_breaks: SoftBreaks::Join,
                },
            );
            joined
                .first()
                .map(|l| vec![truncate_line(l, width, &opts.glyphs.ellipsis)])
                .unwrap_or_default()
        }
    }
}

struct Frame<'a> {
    widths: &'a [usize],
    padding: usize,
    borders: Borders,
    style: Style,
}

impl Frame<'_> {
    fn rule(&self, left: char, mid: char, right: char) -> Line {
        let mut s = String::new();
        s.push(left);
        for (i, w) in self.widths.iter().enumerate() {
            if i > 0 {
                s.push(mid);
            }
            for _ in 0..(w + 2 * self.padding) {
                s.push(self.borders.h);
            }
        }
        s.push(right);
        vec![Segment::new(s, self.style)]
    }

    fn row(&self, cells: &[Vec<Line>], align: &[Align], extra: Style) -> Vec<Line> {
        let height = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let sep = Segment::new(self.borders.v.to_string(), self.style);
        let gap = Segment::new(" ".repeat(self.padding), self.style);
        (0..height)
            .map(|r| {
                let mut line: Line = vec![sep.clone()];
                for (i, w) in self.widths.iter().enumerate() {
                    let mut content: Line = cells
                        .get(i)
                        .and_then(|c| c.get(r))
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|mut s| {
                            s.style = s.style.merge(extra);
                            s
                        })
                        .collect();
                    pad(
                        &mut content,
                        *w,
                        align.get(i).copied().unwrap_or(Align::Left),
                        Style::PLAIN,
                    );
                    if self.padding > 0 {
                        line.push(gap.clone());
                    }
                    line.extend(content);
                    if self.padding > 0 {
                        line.push(gap.clone());
                    }
                    line.push(sep.clone());
                }
                line
            })
            .collect()
    }
}

/// Renders the table into lines no wider than `available` when the content allows it.
pub fn render(table: &Table, available: usize, opts: &RenderOptions) -> Vec<Line> {
    let cols = table.columns();
    if cols == 0 {
        return Vec::new();
    }
    let (min, natural) = measure(table);
    let chrome = (cols + 1) + cols * 2 * opts.table_padding;
    let content = available.saturating_sub(chrome);
    let (widths, _) = distribute(&min, &natural, content);
    let frame = Frame {
        widths: &widths,
        padding: opts.table_padding,
        borders: opts.glyphs.borders,
        style: opts.theme.table_border,
    };
    let b = opts.glyphs.borders;
    let layout_row = |row: &[InlineText]| -> Vec<Vec<Line>> {
        (0..cols)
            .map(|i| {
                row.get(i)
                    .map(|c| cell_lines(c, widths[i], opts))
                    .unwrap_or_default()
            })
            .collect()
    };
    let align: Vec<Align> = (0..cols)
        .map(|i| table.align.get(i).copied().unwrap_or(Align::Left))
        .collect();

    let mut out = vec![frame.rule(b.top_left, b.top_mid, b.top_right)];
    if !table.head.is_empty() {
        out.extend(frame.row(&layout_row(&table.head), &align, opts.theme.table_header));
        out.push(frame.rule(b.mid_left, b.cross, b.mid_right));
    }
    for row in &table.rows {
        out.extend(frame.row(&layout_row(row), &align, Style::PLAIN));
    }
    out.push(frame.rule(b.bottom_left, b.bottom_mid, b.bottom_right));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wrap::display_width;

    #[test]
    fn distribute_fits() {
        assert_eq!(distribute(&[1, 1], &[5, 7], 20), (vec![5, 7], false));
    }

    #[test]
    fn distribute_shrinks_wide_columns_first() {
        let (w, over) = distribute(&[2, 3], &[4, 20], 14);
        assert!(!over);
        assert_eq!(w.iter().sum::<usize>(), 14);
        assert_eq!(w[0], 4);
        assert_eq!(w[1], 10);
    }

    #[test]
    fn distribute_below_min_is_proportional() {
        let (w, over) = distribute(&[10, 10], &[10, 10], 10);
        assert!(over);
        assert_eq!(w, vec![5, 5]);
    }

    #[test]
    fn distribute_never_zero() {
        let (w, _) = distribute(&[1, 100], &[1, 100], 3);
        assert!(w.iter().all(|&x| x >= 1));
    }

    #[test]
    fn unicode_borders_are_wide_one() {
        for c in [UNICODE_BORDERS.h, UNICODE_BORDERS.v, UNICODE_BORDERS.cross] {
            assert_eq!(display_width(&c.to_string()), 1);
        }
    }
}
