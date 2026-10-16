//! Flowcharts: parsing, layered layout and drawing.

use std::collections::HashMap;

use super::canvas::{Canvas, Dir};
use super::route::{Blocked, route};
use crate::model::Line;
use crate::options::RenderOptions;
use crate::wrap::segments_width;

/// Blank cells kept around the layout so that edges can route around the outer nodes.
const MARGIN: usize = 2;
/// Border plus one blank cell on each side of a label.
const NODE_PAD: usize = 4;
const NODE_HEIGHT: usize = 3;
/// Blank cells between two layers, along the flow.
const GAP_ALONG_VERTICAL: usize = 3;
const GAP_ALONG_HORIZONTAL: usize = 5;
/// Blank cells between two nodes of a layer.
const GAP_ACROSS_VERTICAL: usize = 4;
const GAP_ACROSS_HORIZONTAL: usize = 2;
/// Passes of neighbour-average ordering over the layers.
const ORDER_PASSES: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

impl Direction {
    fn parse(token: &str) -> Option<Direction> {
        match token {
            "TD" | "TB" => Some(Direction::TopDown),
            "BT" => Some(Direction::BottomUp),
            "LR" => Some(Direction::LeftRight),
            "RL" => Some(Direction::RightLeft),
            _ => None,
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Direction::LeftRight | Direction::RightLeft)
    }

    /// Direction in which edges between layers leave a node.
    fn flow(self) -> Dir {
        match self {
            Direction::TopDown => Dir::S,
            Direction::BottomUp => Dir::N,
            Direction::LeftRight => Dir::E,
            Direction::RightLeft => Dir::W,
        }
    }

    /// Side used by edges that go back to an earlier layer.
    fn side(self) -> Dir {
        if self.horizontal() { Dir::S } else { Dir::E }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Node {
    label: String,
}

#[derive(Debug, PartialEq, Eq)]
struct Edge {
    from: usize,
    to: usize,
    arrow: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Flow {
    direction: Direction,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

/// Parses a `graph` or `flowchart` block. `None` for another diagram type and for syntax that is
/// not drawn.
pub fn parse(src: &str) -> Option<Flow> {
    let mut lines = src
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let mut header = lines.next()?.trim_end_matches(';').split_whitespace();
    if !matches!(header.next()?, "graph" | "flowchart") {
        return None;
    }
    let direction = match header.next() {
        Some(token) => Direction::parse(token)?,
        None => Direction::TopDown,
    };
    if header.next().is_some() {
        return None;
    }
    let mut flow = Flow {
        direction,
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let mut ids: HashMap<String, usize> = HashMap::new();
    for line in lines {
        for statement in line.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let word = statement.split_whitespace().next().unwrap_or("");
            match word {
                "style" | "classDef" | "class" | "linkStyle" | "click" => continue,
                "subgraph" | "end" | "direction" => return None,
                _ => {}
            }
            parse_statement(statement, &mut flow, &mut ids)?;
        }
    }
    Some(flow)
}

/// One chain such as `A[Start] --> B --> C`.
fn parse_statement(s: &str, flow: &mut Flow, ids: &mut HashMap<String, usize>) -> Option<()> {
    let mut rest = s;
    let mut pending: Option<(usize, bool)> = None;
    loop {
        let (node, after) = parse_node(rest, flow, ids)?;
        if let Some((from, arrow)) = pending.take() {
            flow.edges.push(Edge {
                from,
                to: node,
                arrow,
            });
        }
        rest = after.trim_start();
        if rest.is_empty() {
            return Some(());
        }
        let (arrow, after) = parse_edge(rest)?;
        pending = Some((node, arrow));
        rest = after.trim_start();
    }
}

fn parse_node<'a>(
    s: &'a str,
    flow: &mut Flow,
    ids: &mut HashMap<String, usize>,
) -> Option<(usize, &'a str)> {
    let end = s
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    let (id, mut rest) = s.split_at(end);
    if id.is_empty() {
        return None;
    }
    let mut label = None;
    if let Some(inner) = rest.strip_prefix('[') {
        let close = inner.find(']')?;
        label = Some(inner[..close].trim().trim_matches('"').to_string());
        rest = &inner[close + 1..];
    }
    let index = *ids.entry(id.to_string()).or_insert_with(|| {
        flow.nodes.push(Node {
            label: id.to_string(),
        });
        flow.nodes.len() - 1
    });
    if let Some(label) = label.filter(|l| !l.is_empty()) {
        flow.nodes[index].label = label;
    }
    Some((index, rest))
}

/// Returns whether the edge ends with an arrow head, and the text after it.
fn parse_edge(s: &str) -> Option<(bool, &str)> {
    if let Some(rest) = s.strip_prefix("-->") {
        Some((true, rest))
    } else {
        s.strip_prefix("---").map(|rest| (false, rest))
    }
}

struct Placed {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Placed {
    /// Cell of the border on `side` where an edge attaches. With `other`, the middle of the span the
    /// two boxes share along that border when there is one, so that the edge runs straight;
    /// without, the middle of the border.
    fn port(&self, side: Dir, other: Option<&Placed>) -> (usize, usize) {
        let span = |start: usize, len: usize, o_start: usize, o_len: usize| {
            let lo = start.max(o_start) + 1;
            let hi = (start + len).min(o_start + o_len).saturating_sub(2);
            if lo <= hi {
                (lo + hi) / 2
            } else {
                start + len / 2
            }
        };
        let col = other.map_or(self.x + self.w / 2, |o| span(self.x, self.w, o.x, o.w));
        let row = other.map_or(self.y + self.h / 2, |o| span(self.y, self.h, o.y, o.h));
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

/// Places the nodes layer by layer. Returns the boxes and the size of the layout.
fn place(flow: &Flow, layers: &[Vec<usize>]) -> (Vec<Placed>, usize, usize) {
    let horizontal = flow.direction.horizontal();
    let (gap_along, gap_across) = if horizontal {
        (GAP_ALONG_HORIZONTAL, GAP_ACROSS_HORIZONTAL)
    } else {
        (GAP_ALONG_VERTICAL, GAP_ACROSS_VERTICAL)
    };
    let sizes: Vec<(usize, usize)> = flow
        .nodes
        .iter()
        .map(|n| (label_width(&n.label) + NODE_PAD, NODE_HEIGHT))
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
    let total_along = spans.iter().sum::<usize>() + gap_along * layers.len().saturating_sub(1);
    let mirrored = matches!(flow.direction, Direction::BottomUp | Direction::RightLeft);
    let mut placed: Vec<Placed> = sizes
        .iter()
        .map(|&(w, h)| Placed { x: 0, y: 0, w, h })
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
        offset += spans[r] + gap_along;
    }
    let (w, h) = if horizontal {
        (total_along, total_across)
    } else {
        (total_across, total_along)
    };
    (placed, w, h)
}

fn label_width(label: &str) -> usize {
    crate::wrap::display_width(label)
}

/// Draws the flowchart within `width` columns. `None` when it does not fit or cannot be routed.
pub fn draw(flow: &Flow, width: usize, opts: &RenderOptions) -> Option<Vec<Line>> {
    if flow.nodes.is_empty() {
        return None;
    }
    let back = back_edges(flow)?;
    let rank = ranks(flow, &back);
    let layers = layers(flow, &back, &rank);
    let (placed, w, h) = place(flow, &layers);
    let (cw, ch) = (w + 2 * MARGIN, h + 2 * MARGIN);
    let mut canvas = Canvas::new(cw, ch);
    let mut blocked = Blocked::new(cw, ch);
    for (node, p) in flow.nodes.iter().zip(&placed) {
        canvas.draw_box(p.x, p.y, p.w, p.h);
        canvas.text(p.x + 2, p.y + p.h / 2, &node.label);
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
        canvas.add_lines(source.0, source.1, out.bit());
        let last = path.len() - 1;
        for (i, &cell) in path.iter().enumerate() {
            let before = if i == 0 {
                out.opposite()
            } else {
                Dir::between(cell, path[i - 1])
            };
            if i < last {
                let after = Dir::between(cell, path[i + 1]);
                canvas.add_lines(cell.0, cell.1, before.bit() | after.bit());
            } else if edge.arrow {
                canvas.set_glyph(cell.0, cell.1, opts.glyphs.arrows[arrive.index()]);
            } else {
                canvas.add_lines(cell.0, cell.1, before.bit() | arrive.bit());
                canvas.add_lines(target.0, target.1, in_side.bit());
            }
        }
    }
    let lines = canvas.into_lines(
        &opts.glyphs.borders,
        opts.theme.table_border,
        opts.theme.base,
    );
    if lines.iter().any(|l| segments_width(l) > width) {
        return None;
    }
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parses_a_chain() {
        let flow = parse("graph LR\n  A[Start] --> B --- C;\n  B --> A\n").unwrap();
        assert_eq!(flow.direction, Direction::LeftRight);
        assert_eq!(flow.nodes.len(), 3);
        assert_eq!(flow.nodes[0].label, "Start");
        assert_eq!(flow.nodes[1].label, "B");
        assert_eq!(flow.edges.len(), 3);
        assert!(flow.edges[0].arrow && !flow.edges[1].arrow);
    }

    #[test]
    fn rejects_other_diagrams_and_unsupported_syntax() {
        assert!(parse("sequenceDiagram\n A->>B: x").is_none());
        assert!(parse("graph TD\n A --> B\n subgraph s\n end").is_none());
        assert!(parse("graph TD\n A((x)) --> B").is_none());
        assert!(parse("graph XX\n A --> B").is_none());
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
