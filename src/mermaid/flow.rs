//! Flowcharts: layered layout, edge routing and drawing.

use std::collections::HashMap;

use super::canvas::{Canvas, Dir, E, N, S, W, Weight};
use super::graph::{Direction, Flow, Head, Node, Shape};
use super::route::{Blocked, route};
use crate::model::Line;
use crate::options::RenderOptions;
use crate::wrap::{display_width, segments_width};

/// Blank cells kept around the layout so that edges can route around the outer nodes.
const MARGIN: usize = 2;
/// Border plus one blank cell on each side of a label.
const NODE_PAD: usize = 4;
/// Blank cells between two layers, along the flow.
const GAP_ALONG_VERTICAL: usize = 3;
const GAP_ALONG_HORIZONTAL: usize = 5;
/// Cells added to the width of an edge label to get the gap that holds it.
const LABEL_ROOM: usize = 4;
/// Rows added to a vertical gap that holds an edge label.
const LABEL_ROWS: usize = 2;
/// Blank cells between two nodes of a layer.
const GAP_ACROSS_VERTICAL: usize = 4;
const GAP_ACROSS_HORIZONTAL: usize = 2;
/// Left and right points of a diamond node, on the row of its port.
const DIAMOND_SIDES: [char; 2] = ['<', '>'];
/// Cells between the left edge of a diamond and the first cell of its top and bottom edges.
const DIAMOND_INSET: usize = 3;
/// Rows of a diamond above and below its text.
const DIAMOND_SLANT: usize = 2;
/// Passes of neighbour-average ordering over the layers.
const ORDER_PASSES: usize = 2;

struct Placed {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    diamond: bool,
}

impl Placed {
    /// Columns of the top and bottom border where an edge may attach.
    fn columns(&self) -> (usize, usize) {
        let inset = if self.diamond { DIAMOND_INSET } else { 1 };
        (self.x + inset, (self.x + self.w).saturating_sub(inset + 1))
    }

    /// Rows of the left and right border where an edge may attach.
    fn rows(&self) -> (usize, usize) {
        if self.diamond {
            (self.y + self.h / 2, self.y + self.h / 2)
        } else {
            (self.y + 1, (self.y + self.h).saturating_sub(2))
        }
    }

    /// Cell of the border on `side` where an edge attaches. With `other`, the middle of the span the
    /// two boxes share along that border when there is one, so that the edge runs straight;
    /// without, the middle of the border.
    fn port(&self, side: Dir, other: Option<&Placed>) -> (usize, usize) {
        let span = |own: (usize, usize), theirs: (usize, usize), middle: usize| {
            let (lo, hi) = (own.0.max(theirs.0), own.1.min(theirs.1));
            if lo <= hi { (lo + hi) / 2 } else { middle }
        };
        let col = other.map_or(self.x + self.w / 2, |o| {
            span(self.columns(), o.columns(), self.x + self.w / 2)
        });
        let row = other.map_or(self.y + self.h / 2, |o| {
            span(self.rows(), o.rows(), self.y + self.h / 2)
        });
        let row = if self.diamond { self.rows().0 } else { row };
        match side {
            Dir::N => (col, self.y),
            Dir::S => (col, self.y + self.h - 1),
            Dir::E => (self.x + self.w - 1, row),
            Dir::W => (self.x, row),
        }
    }
}

/// Edges closing a cycle in a depth-first walk from the nodes in order. `None` for a self-loop.
fn back_edges(flow: &Flow) -> Option<Vec<bool>> {
    let n = flow.nodes.len();
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in flow.edges.iter().enumerate() {
        if e.from == e.to {
            return None;
        }
        out[e.from].push(i);
    }
    let mut color = vec![0u8; n];
    let mut back = vec![false; flow.edges.len()];
    for root in 0..n {
        if color[root] != 0 {
            continue;
        }
        color[root] = 1;
        let mut stack = vec![(root, 0usize)];
        while let Some(top) = stack.last_mut() {
            let (u, i) = *top;
            if i == out[u].len() {
                color[u] = 2;
                stack.pop();
                continue;
            }
            top.1 += 1;
            let e = out[u][i];
            let v = flow.edges[e].to;
            match color[v] {
                0 => {
                    color[v] = 1;
                    stack.push((v, 0));
                }
                1 => back[e] = true,
                _ => {}
            }
        }
    }
    Some(back)
}

/// Layer of each node: the longest path from a node without incoming edge.
fn ranks(flow: &Flow, back: &[bool]) -> Vec<usize> {
    let mut rank = vec![0usize; flow.nodes.len()];
    loop {
        let mut changed = false;
        for (e, edge) in flow.edges.iter().enumerate() {
            if !back[e] && rank[edge.to] <= rank[edge.from] {
                rank[edge.to] = rank[edge.from] + 1;
                changed = true;
            }
        }
        if !changed {
            return rank;
        }
    }
}

/// Orders each layer by the average position of the neighbours in `nbrs`.
fn reorder(layer: &mut [usize], pos: &mut [usize], nbrs: &[Vec<usize>]) {
    let key = |v: usize| {
        if nbrs[v].is_empty() {
            pos[v] as f64
        } else {
            nbrs[v].iter().map(|&u| pos[u] as f64).sum::<f64>() / nbrs[v].len() as f64
        }
    };
    let mut keyed: Vec<(f64, usize)> = layer.iter().map(|&v| (key(v), v)).collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (i, (_, v)) in keyed.into_iter().enumerate() {
        layer[i] = v;
        pos[v] = i;
    }
}

fn layers(flow: &Flow, back: &[bool], rank: &[usize]) -> Vec<Vec<usize>> {
    let depth = rank.iter().max().map_or(0, |m| m + 1);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); depth];
    let mut pos = vec![0usize; flow.nodes.len()];
    for (v, &r) in rank.iter().enumerate() {
        pos[v] = layers[r].len();
        layers[r].push(v);
    }
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); flow.nodes.len()];
    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); flow.nodes.len()];
    for (e, edge) in flow.edges.iter().enumerate() {
        if !back[e] {
            preds[edge.to].push(edge.from);
            succs[edge.from].push(edge.to);
        }
    }
    for _ in 0..ORDER_PASSES {
        for layer in layers.iter_mut().skip(1) {
            reorder(layer, &mut pos, &preds);
        }
        for layer in layers.iter_mut().rev().skip(1) {
            reorder(layer, &mut pos, &succs);
        }
    }
    layers
}

/// Blank cells after each layer. A gap holds the label of an edge to the next layer: horizontal
/// flows widen it by the label width, vertical flows by `LABEL_ROWS` rows.
fn gaps(flow: &Flow, back: &[bool], rank: &[usize], depth: usize) -> Vec<usize> {
    let horizontal = flow.direction.horizontal();
    let base = if horizontal {
        GAP_ALONG_HORIZONTAL
    } else {
        GAP_ALONG_VERTICAL
    };
    let mut gaps = vec![base; depth.saturating_sub(1)];
    for (e, edge) in flow.edges.iter().enumerate() {
        if let (Some(label), false) = (&edge.label, back[e]) {
            let r = rank[edge.from];
            gaps[r] = gaps[r].max(if horizontal {
                display_width(label) + LABEL_ROOM
            } else {
                base + LABEL_ROWS
            });
        }
    }
    gaps
}

/// Writes `label` beside the longest straight run of `path` that has room for it.
fn place_label(canvas: &mut Canvas, path: &[(usize, usize)], label: &str) -> bool {
    let width = display_width(label);
    let mut runs: Vec<&[(usize, usize)]> = Vec::new();
    let mut start = 0;
    for i in 1..path.len() {
        let turns = i + 1 < path.len()
            && Dir::between(path[i - 1], path[i]) != Dir::between(path[i], path[i + 1]);
        if turns || i + 1 == path.len() {
            runs.push(&path[start..=i]);
            start = i;
        }
    }
    runs.sort_by_key(|r| std::cmp::Reverse(r.len()));
    // First with a blank cell on each side of the label, then without. Along a run, the
    // positions closest to its middle come first.
    for margin in [1, 0] {
        for run in &runs {
            let (first, last) = (run[0], run[run.len() - 1]);
            let mut candidates: Vec<(usize, usize, usize)> = Vec::new();
            if first.1 == last.1 {
                let (lo, hi) = (first.0.min(last.0), first.0.max(last.0));
                let middle = (lo + hi) / 2;
                for x in lo..=hi {
                    let distance = (x + width / 2).abs_diff(middle);
                    candidates.push((distance, x, first.1.wrapping_sub(1)));
                    candidates.push((distance, x, first.1 + 1));
                }
            } else {
                let (lo, hi) = (first.1.min(last.1), first.1.max(last.1));
                let middle = (lo + hi) / 2;
                for y in lo..=hi {
                    let distance = y.abs_diff(middle);
                    candidates.push((distance, first.0 + 1 + margin, y));
                    candidates.push((distance, first.0.wrapping_sub(width + margin), y));
                }
            }
            candidates.sort_by_key(|&(distance, ..)| distance);
            for (_, x, y) in candidates {
                let fits = x >= margin
                    && x.checked_add(width + margin)
                        .is_some_and(|end| end < canvas.width())
                    && y < canvas.height()
                    && (x - margin..x + width + margin).all(|cx| canvas.is_empty(cx, y));
                if fits {
                    canvas.text(x, y, label);
                    return true;
                }
            }
        }
    }
    false
}

/// Places the nodes layer by layer. Returns the boxes and the size of the layout.
fn place(
    flow: &Flow,
    texts: &[Vec<String>],
    layers: &[Vec<usize>],
    gaps: &[usize],
) -> (Vec<Placed>, usize, usize) {
    let horizontal = flow.direction.horizontal();
    let gap_across = if horizontal {
        GAP_ACROSS_HORIZONTAL
    } else {
        GAP_ACROSS_VERTICAL
    };
    let sizes: Vec<(usize, usize)> = flow
        .nodes
        .iter()
        .zip(texts)
        .map(|(n, lines)| {
            let text = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
            if n.shape == Shape::Diamond {
                (text + 2 * DIAMOND_INSET, lines.len() + 2 * DIAMOND_SLANT)
            } else {
                (text + NODE_PAD, lines.len() + 2 + n.dividers.len())
            }
        })
        .collect();
    let along = |v: usize| if horizontal { sizes[v].0 } else { sizes[v].1 };
    let across = |v: usize| if horizontal { sizes[v].1 } else { sizes[v].0 };
    let spans: Vec<usize> = layers
        .iter()
        .map(|l| l.iter().map(|&v| along(v)).max().unwrap_or(0))
        .collect();
    let widths: Vec<usize> = layers
        .iter()
        .map(|l| {
            l.iter().map(|&v| across(v)).sum::<usize>() + gap_across * l.len().saturating_sub(1)
        })
        .collect();
    let total_across = widths.iter().copied().max().unwrap_or(0);
    let total_along = spans.iter().sum::<usize>() + gaps.iter().sum::<usize>();
    let mirrored = matches!(flow.direction, Direction::BottomUp | Direction::RightLeft);
    let mut placed: Vec<Placed> = sizes
        .iter()
        .zip(&flow.nodes)
        .map(|(&(w, h), n)| Placed {
            x: 0,
            y: 0,
            w,
            h,
            diamond: n.shape == Shape::Diamond,
        })
        .collect();
    let mut offset = 0;
    for (r, layer) in layers.iter().enumerate() {
        let mut cross = (total_across - widths[r]) / 2;
        for &v in layer {
            let mut main = offset + (spans[r] - along(v)) / 2;
            if mirrored {
                main = total_along - main - along(v);
            }
            let (x, y) = if horizontal {
                (main, cross)
            } else {
                (cross, main)
            };
            placed[v].x = x + MARGIN;
            placed[v].y = y + MARGIN;
            cross += across(v) + gap_across;
        }
        offset += spans[r] + gaps.get(r).copied().unwrap_or(0);
    }
    let (w, h) = if horizontal {
        (total_along, total_across)
    } else {
        (total_across, total_along)
    };
    (placed, w, h)
}

/// Text of a node: its lines, or the glyph of a start or end state.
fn node_text(node: &Node, opts: &RenderOptions) -> Vec<String> {
    match node.shape {
        Shape::Start => vec![opts.glyphs.start_state.clone()],
        Shape::End => vec![opts.glyphs.end_state.clone()],
        _ => node.lines.clone(),
    }
}

/// Draws a node: its border, whose corners depend on the shape, and its text.
fn draw_node(canvas: &mut Canvas, node: &Node, text: &[String], p: &Placed, opts: &RenderOptions) {
    if node.shape == Shape::Diamond {
        return draw_diamond(canvas, text, p, opts);
    }
    canvas.draw_box(p.x, p.y, p.w, p.h);
    if matches!(node.shape, Shape::Round | Shape::Start | Shape::End) {
        let [tl, tr, bl, br] = opts.glyphs.rounded;
        canvas.set_glyph(p.x, p.y, tl);
        canvas.set_glyph(p.x + p.w - 1, p.y, tr);
        canvas.set_glyph(p.x, p.y + p.h - 1, bl);
        canvas.set_glyph(p.x + p.w - 1, p.y + p.h - 1, br);
    }
    let first_divider = node.dividers.first().copied().unwrap_or(usize::MAX);
    let mut y = p.y + 1;
    for (i, line) in text.iter().enumerate() {
        if node.dividers.contains(&i) {
            canvas.add_lines(p.x, y, N | S | E, Weight::Normal);
            canvas.add_lines(p.x + p.w - 1, y, N | S | W, Weight::Normal);
            for x in p.x + 1..p.x + p.w - 1 {
                canvas.add_lines(x, y, E | W, Weight::Normal);
            }
            y += 1;
        }
        let pad = if i < first_divider {
            (p.w - 2 - display_width(line)) / 2
        } else {
            1
        };
        canvas.text(p.x + 1 + pad, y, line);
        y += 1;
    }
}

/// Draws a diamond: two slanted rows above and below the label, pointed on its row.
fn draw_diamond(canvas: &mut Canvas, text: &[String], p: &Placed, opts: &RenderOptions) {
    let [tl, tr, bl, br] = opts.glyphs.diamond;
    let (left, right) = (p.x, p.x + p.w - 1);
    let (top, bottom) = (p.y, p.y + p.h - 1);
    for x in left + DIAMOND_INSET..right + 1 - DIAMOND_INSET {
        canvas.add_lines(x, top, E | W, Weight::Normal);
        canvas.add_lines(x, bottom, E | W, Weight::Normal);
    }
    canvas.set_glyph(left + 2, top, tl);
    canvas.set_glyph(right - 2, top, tr);
    canvas.set_glyph(left + 2, bottom, bl);
    canvas.set_glyph(right - 2, bottom, br);
    canvas.set_glyph(left + 1, top + 1, tl);
    canvas.set_glyph(right - 1, top + 1, tr);
    canvas.set_glyph(left + 1, bottom - 1, bl);
    canvas.set_glyph(right - 1, bottom - 1, br);
    let tip = p.rows().0;
    for (i, line) in text.iter().enumerate() {
        let y = top + DIAMOND_SLANT + i;
        if y == tip {
            canvas.set_glyph(left, y, DIAMOND_SIDES[0]);
            canvas.set_glyph(right, y, DIAMOND_SIDES[1]);
        } else {
            canvas.add_lines(left, y, N | S, Weight::Normal);
            canvas.add_lines(right, y, N | S, Weight::Normal);
        }
        let pad = (p.w - 2 * DIAMOND_INSET - display_width(line)) / 2;
        canvas.text(left + DIAMOND_INSET + pad, y, line);
    }
}

/// The glyph of an edge end that points along `heading`, if the end has a head.
fn head_glyph(head: Head, heading: Dir, opts: &RenderOptions) -> Option<char> {
    match head {
        Head::None => None,
        Head::Arrow => Some(opts.glyphs.arrows[heading.index()]),
        Head::Inherit => Some(opts.glyphs.inherit[heading.index()]),
        Head::Composition => Some(opts.glyphs.composition),
        Head::Aggregation => Some(opts.glyphs.aggregation),
    }
}

/// Draws the flowchart within `width` columns. `None` when it does not fit or cannot be routed.
pub fn draw(flow: &Flow, width: usize, opts: &RenderOptions) -> Option<Vec<Line>> {
    if flow.nodes.is_empty() {
        return None;
    }
    let back = back_edges(flow)?;
    let rank = ranks(flow, &back);
    let layers = layers(flow, &back, &rank);
    let gaps = gaps(flow, &back, &rank, layers.len());
    let texts: Vec<Vec<String>> = flow.nodes.iter().map(|n| node_text(n, opts)).collect();
    let (placed, w, h) = place(flow, &texts, &layers, &gaps);
    // In a vertical flow a label is written beside its edge and may run past the layout.
    let room = if flow.direction.horizontal() {
        0
    } else {
        flow.edges
            .iter()
            .filter_map(|edge| edge.label.as_ref())
            .map(|label| display_width(label) + 3)
            .max()
            .unwrap_or(0)
    };
    let (cw, ch) = (w + 2 * MARGIN + room, h + 2 * MARGIN);
    let mut canvas = Canvas::new(cw, ch);
    let mut blocked = Blocked::new(cw, ch);
    for ((node, text), p) in flow.nodes.iter().zip(&texts).zip(&placed) {
        draw_node(&mut canvas, node, text, p, opts);
        blocked.block_rect(p.x, p.y, p.w, p.h);
    }
    let flow_dir = flow.direction.flow();
    let side = flow.direction.side();
    let sides: Vec<(Dir, Dir)> = (0..flow.edges.len())
        .map(|e| {
            if back[e] {
                (side, side)
            } else {
                (flow_dir, flow_dir.opposite())
            }
        })
        .collect();
    let mut uses: HashMap<(usize, usize), usize> = HashMap::new();
    for (edge, &(out, in_side)) in flow.edges.iter().zip(&sides) {
        *uses.entry((edge.from, out.index())).or_default() += 1;
        *uses.entry((edge.to, in_side.index())).or_default() += 1;
    }
    let alone = |node: usize, side: Dir| uses[&(node, side.index())] == 1;
    for (edge, &(out, in_side)) in flow.edges.iter().zip(&sides) {
        let arrive = in_side.opposite();
        let source = placed[edge.from].port(out, alone(edge.from, out).then_some(&placed[edge.to]));
        let target = placed[edge.to].port(
            in_side,
            alone(edge.to, in_side).then_some(&placed[edge.from]),
        );
        let path = route(
            &canvas,
            &blocked,
            out.step(source),
            out,
            in_side.step(target),
            arrive,
        )?;
        canvas.add_lines(source.0, source.1, out.bit(), Weight::Normal);
        let last = path.len() - 1;
        for (i, &cell) in path.iter().enumerate() {
            let before = if i == 0 {
                out.opposite()
            } else {
                Dir::between(cell, path[i - 1])
            };
            if i < last {
                let after = Dir::between(cell, path[i + 1]);
                canvas.add_lines(cell.0, cell.1, before.bit() | after.bit(), edge.weight);
            } else if let Some(glyph) = head_glyph(edge.to_end, arrive, opts) {
                canvas.set_glyph(cell.0, cell.1, glyph);
            } else {
                canvas.add_lines(cell.0, cell.1, before.bit() | arrive.bit(), edge.weight);
                canvas.add_lines(target.0, target.1, in_side.bit(), Weight::Normal);
            }
        }
        if let Some(glyph) = head_glyph(edge.from_end, out.opposite(), opts)
            && path.len() > 1
        {
            canvas.set_glyph(path[0].0, path[0].1, glyph);
        }
        if let Some(label) = &edge.label
            && !place_label(&mut canvas, &path, label)
        {
            return None;
        }
    }
    let lines = canvas.into_lines(&opts.glyphs, opts.theme.table_border, opts.theme.base);
    if lines.iter().any(|l| segments_width(l) > width) {
        return None;
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::parse::parse;

    fn draw_plain(src: &str, width: usize) -> Option<String> {
        let flow = parse(src)?;
        let lines = draw(&flow, width, &RenderOptions::plain(width))?;
        Some(
            lines
                .iter()
                .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>() + "\n")
                .collect(),
        )
    }

    #[test]
    fn shapes_change_the_corners() {
        let art = draw_plain("graph TD\n A(x) --> B{y}\n", 40).unwrap();
        assert!(art.contains(".---."), "{art}");
    }

    #[test]
    fn diamond_has_slanted_sides_and_pointed_tips() {
        let art = draw_plain("graph TD\n B{yes}\n", 40).unwrap();
        assert_eq!(art, "  /---\\\n /     \\\n<  yes  >\n \\     /\n  \\---/\n");
    }

    #[test]
    fn edge_label_is_written_next_to_the_edge() {
        let art = draw_plain("graph LR\n A -->|ok| B\n", 40).unwrap();
        assert_eq!(
            art,
            "+---+ ok   +---+\n| A +----->| B |\n+---+      +---+\n"
        );
    }

    #[test]
    fn dotted_and_thick_edges_use_their_own_glyphs() {
        let dotted = draw_plain("graph LR\n A -.-> B\n", 40).unwrap();
        assert!(dotted.contains("....>"), "{dotted}");
        let thick = draw_plain("graph LR\n A ==> B\n", 40).unwrap();
        assert!(thick.contains("====>"), "{thick}");
    }

    #[test]
    fn label_without_room_falls_back() {
        assert!(draw_plain("graph TD\n A -->|a very long label| B\n", 12).is_none());
    }

    #[test]
    fn top_down_chain() {
        let art = draw_plain("graph TD\n A[Start] --> B[End]\n", 40).unwrap();
        assert_eq!(
            art,
            "+-------+\n| Start |\n+---+---+\n    |\n    |\n    v\n +-----+\n | End |\n +-----+\n"
        );
    }

    #[test]
    fn left_right_chain() {
        let art = draw_plain("graph LR\n A --> B\n", 40).unwrap();
        assert_eq!(art, "+---+     +---+\n| A +---->| B |\n+---+     +---+\n");
    }

    #[test]
    fn too_wide_falls_back() {
        assert!(draw_plain("graph LR\n A --> B\n", 10).is_none());
    }

    #[test]
    fn cycle_is_drawn() {
        let art = draw_plain("graph TD\n A --> B\n B --> A\n", 40).unwrap();
        assert!(art.contains('A') && art.contains('B'));
    }

    #[test]
    fn self_loop_falls_back() {
        assert!(draw_plain("graph TD\n A --> A\n", 40).is_none());
    }
}
