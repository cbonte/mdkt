//! Flowchart syntax: header, nodes and edges.

use super::canvas::Weight;
use super::graph::{Builder, Direction, Edge, Flow, Head, Node, Shape};

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
    let mut builder = Builder::new(direction);
    for line in lines {
        for statement in line.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            let word = statement.split_whitespace().next().unwrap_or("");
            match word {
                "style" | "classDef" | "class" | "linkStyle" | "click" => continue,
                "subgraph" | "end" | "direction" => return None,
                _ => {}
            }
            parse_statement(statement, &mut builder)?;
        }
    }
    Some(builder.flow)
}

/// One chain such as `A[Start] --> B --> C`.
fn parse_statement(s: &str, b: &mut Builder) -> Option<()> {
    let mut rest = s;
    let mut pending: Option<(usize, Link)> = None;
    loop {
        let (node, after) = parse_node(rest, b)?;
        if let Some((from, link)) = pending.take() {
            b.flow.edges.push(Edge {
                from,
                to: node,
                from_end: Head::None,
                to_end: if link.arrow { Head::Arrow } else { Head::None },
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

fn parse_node<'a>(s: &'a str, b: &mut Builder) -> Option<(usize, &'a str)> {
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
    let index = b.node(id);
    if let Some((lines, shape)) = shaped.filter(|(lines, _)| !lines.is_empty()) {
        b.flow.nodes[index] = Node {
            lines,
            dividers: Vec::new(),
            shape,
        };
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
        assert!(flow.edges[0].to_end == Head::Arrow && flow.edges[1].to_end == Head::None);
    }

    #[test]
    fn parses_edge_styles_and_text() {
        let src = "graph TD\n A -.-> B\n B ==> C\n C -->|yes| D\n D -- no --- E\n E == big ==> F\n";
        let flow = parse(src).unwrap();
        let edges: Vec<_> = flow
            .edges
            .iter()
            .map(|e| (e.to_end == Head::Arrow, e.weight, e.label.as_deref()))
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
