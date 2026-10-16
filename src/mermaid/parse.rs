//! Flowchart syntax: header, nodes and edges.

use std::collections::HashMap;

use super::canvas::{Dir, Weight};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

impl Direction {
    pub fn parse(token: &str) -> Option<Direction> {
        match token {
            "TD" | "TB" => Some(Direction::TopDown),
            "BT" => Some(Direction::BottomUp),
            "LR" => Some(Direction::LeftRight),
            "RL" => Some(Direction::RightLeft),
            _ => None,
        }
    }

    pub fn horizontal(self) -> bool {
        matches!(self, Direction::LeftRight | Direction::RightLeft)
    }

    /// Direction in which edges between layers leave a node.
    pub fn flow(self) -> Dir {
        match self {
            Direction::TopDown => Dir::S,
            Direction::BottomUp => Dir::N,
            Direction::LeftRight => Dir::E,
            Direction::RightLeft => Dir::W,
        }
    }

    /// Side used by edges that go back to an earlier layer.
    pub fn side(self) -> Dir {
        if self.horizontal() { Dir::S } else { Dir::E }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Rect,
    Round,
    Diamond,
}

/// Opening and closing delimiters of the node shapes, longest opening first.
const SHAPES: [(&str, &str, Shape); 9] = [
    ("((", "))", Shape::Round),
    ("([", "])", Shape::Round),
    ("[[", "]]", Shape::Rect),
    ("[(", ")]", Shape::Rect),
    ("{{", "}}", Shape::Rect),
    ("[", "]", Shape::Rect),
    ("(", ")", Shape::Round),
    ("{", "}", Shape::Diamond),
    (">", "]", Shape::Rect),
];

#[derive(Debug, PartialEq, Eq)]
pub struct Node {
    pub lines: Vec<String>,
    pub shape: Shape,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub arrow: bool,
    pub weight: Weight,
    pub label: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Flow {
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
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
    let mut pending: Option<(usize, Link)> = None;
    loop {
        let (node, after) = parse_node(rest, flow, ids)?;
        if let Some((from, link)) = pending.take() {
            flow.edges.push(Edge {
                from,
                to: node,
                arrow: link.arrow,
                weight: link.weight,
                label: link.label,
            });
        }
        rest = after.trim_start();
        if rest.is_empty() {
            return Some(());
        }
        let (link, after) = parse_edge(rest)?;
        pending = Some((node, link));
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
    let mut shaped = None;
    for (open, close, shape) in SHAPES {
        if let Some(inner) = rest.strip_prefix(open) {
            let (label, after) = parse_label(inner, close)?;
            shaped = Some((label, shape));
            rest = after;
            break;
        }
    }
    let index = *ids.entry(id.to_string()).or_insert_with(|| {
        flow.nodes.push(Node {
            lines: vec![id.to_string()],
            shape: Shape::Rect,
        });
        flow.nodes.len() - 1
    });
    if let Some((lines, shape)) = shaped.filter(|(lines, _)| !lines.is_empty()) {
        flow.nodes[index] = Node { lines, shape };
    }
    Some((index, rest))
}

/// Splits `s` at the delimiter `close`, ignoring one inside double quotes. Returns the label as
/// lines, `<br>` starting a new one, and the text after the delimiter.
fn parse_label<'a>(s: &'a str, close: &str) -> Option<(Vec<String>, &'a str)> {
    let s = s.trim_start();
    let (text, after) = if let Some(quoted) = s.strip_prefix('"') {
        let end = quoted.find('"')?;
        let after = quoted[end + 1..].trim_start().strip_prefix(close)?;
        (&quoted[..end], after)
    } else {
        let end = s.find(close)?;
        (&s[..end], &s[end + close.len()..])
    };
    let text = text
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n");
    let lines = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Some((lines, after))
}

struct Link {
    arrow: bool,
    weight: Weight,
    label: Option<String>,
}

/// Links without text: operator, arrow head, weight.
const PLAIN_LINKS: [(&str, bool, Weight); 6] = [
    ("-->", true, Weight::Normal),
    ("---", false, Weight::Normal),
    ("-.->", true, Weight::Dotted),
    ("-.-", false, Weight::Dotted),
    ("==>", true, Weight::Thick),
    ("===", false, Weight::Thick),
];

/// Closing operators of a link with text, each with its arrow head.
type Closes = [(&'static str, bool); 2];

/// Links with text between an opening and a closing operator: `-- text -->`.
const TEXT_LINKS: [(&str, Closes, Weight); 3] = [
    ("--", [("-->", true), ("---", false)], Weight::Normal),
    ("-.", [(".->", true), (".-", false)], Weight::Dotted),
    ("==", [("==>", true), ("===", false)], Weight::Thick),
];

/// A link, with its text either inside the operator or between bars after it, and the text that
/// follows.
fn parse_edge(s: &str) -> Option<(Link, &str)> {
    let mut link = None;
    for (op, arrow, weight) in PLAIN_LINKS {
        if let Some(rest) = s.strip_prefix(op) {
            link = Some((
                Link {
                    arrow,
                    weight,
                    label: None,
                },
                rest,
            ));
            break;
        }
    }
    if link.is_none() {
        for (open, closes, weight) in TEXT_LINKS {
            let Some(rest) = s.strip_prefix(open).filter(|r| r.starts_with(' ')) else {
                continue;
            };
            let (at, close, arrow) = closes
                .iter()
                .filter_map(|&(c, a)| rest.find(c).map(|at| (at, c, a)))
                .min_by_key(|&(at, c, _)| (at, std::cmp::Reverse(c.len())))?;
            let label = single_line(&rest[..at]);
            link = Some((
                Link {
                    arrow,
                    weight,
                    label,
                },
                &rest[at + close.len()..],
            ));
            break;
        }
    }
    let (mut link, rest) = link?;
    let rest = rest.trim_start();
    match rest.strip_prefix('|') {
        Some(inner) => {
            let end = inner.find('|')?;
            link.label = single_line(&inner[..end]);
            Some((link, &inner[end + 1..]))
        }
        None => Some((link, rest)),
    }
}

/// Edge text on one line, without quotes. `None` when empty.
fn single_line(text: &str) -> Option<String> {
    let text = text
        .replace("<br/>", " ")
        .replace("<br />", " ")
        .replace("<br>", " ");
    let text = text.trim().trim_matches('"').trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_chain() {
        let flow = parse("graph LR\n  A[Start] --> B --- C;\n  B --> A\n").unwrap();
        assert_eq!(flow.direction, Direction::LeftRight);
        assert_eq!(flow.nodes.len(), 3);
        assert_eq!(flow.nodes[0].lines, ["Start"]);
        assert_eq!(flow.nodes[1].lines, ["B"]);
        assert_eq!(flow.edges.len(), 3);
        assert!(flow.edges[0].arrow && !flow.edges[1].arrow);
    }

    #[test]
    fn parses_edge_styles_and_text() {
        let src = "graph TD\n A -.-> B\n B ==> C\n C -->|yes| D\n D -- no --- E\n E == big ==> F\n";
        let flow = parse(src).unwrap();
        let edges: Vec<_> = flow
            .edges
            .iter()
            .map(|e| (e.arrow, e.weight, e.label.as_deref()))
            .collect();
        assert_eq!(
            edges,
            [
                (true, Weight::Dotted, None),
                (true, Weight::Thick, None),
                (true, Weight::Normal, Some("yes")),
                (false, Weight::Normal, Some("no")),
                (true, Weight::Thick, Some("big")),
            ]
        );
    }

    #[test]
    fn rejects_other_diagrams_and_unsupported_syntax() {
        assert!(parse("sequenceDiagram\n A->>B: x").is_none());
        assert!(parse("graph TD\n A --> B\n subgraph s\n end").is_none());
        assert!(parse("graph TD\n A[x --> B").is_none());
        assert!(parse("graph XX\n A --> B").is_none());
    }

    #[test]
    fn parses_shapes_and_labels() {
        let flow = parse("graph TD\n A([Go]) --> B{\"Is it?\"} --> C[a<br/>b]\n").unwrap();
        assert_eq!(flow.nodes[0].shape, Shape::Round);
        assert_eq!(flow.nodes[1].shape, Shape::Diamond);
        assert_eq!(flow.nodes[1].lines, ["Is it?"]);
        assert_eq!(flow.nodes[2].lines, ["a", "b"]);
    }
}
