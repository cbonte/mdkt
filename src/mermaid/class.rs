//! Class diagrams: parsed into the graph drawn by the flowchart layout.

use std::collections::HashMap;

use super::canvas::Weight;
use super::graph::{Builder, Direction, Edge, Flow, Head};

/// Text of a class box, in its three compartments.
#[derive(Default)]
struct Members {
    annotations: Vec<String>,
    fields: Vec<String>,
    methods: Vec<String>,
}

struct Parser {
    b: Builder,
    members: HashMap<usize, Members>,
}

/// `Name~T~` is shown as `Name<T>`.
fn display_name(id: &str) -> String {
    match id.split_once('~') {
        Some((name, rest)) => match rest.strip_suffix('~') {
            Some(inner) => format!("{name}<{inner}>"),
            None => id.to_string(),
        },
        None => id.to_string(),
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '~')
}

impl Parser {
    fn class(&mut self, id: &str) -> Option<usize> {
        if !valid_id(id) {
            return None;
        }
        let index = self.b.node(id);
        self.b.flow.nodes[index].lines[0] = display_name(id);
        Some(index)
    }

    fn add_member(&mut self, index: usize, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let members = self.members.entry(index).or_default();
        if text.starts_with("<<") {
            members.annotations.push(text.to_string());
        } else if text.contains('(') {
            members.methods.push(text.to_string());
        } else {
            members.fields.push(text.to_string());
        }
    }

    fn relation(&mut self, line: &str, at: usize, dotted: bool) -> Option<()> {
        let (before, after) = (&line[..at], &line[at + 2..]);
        let (before, from_end) = if let Some(rest) = before.strip_suffix("<|") {
            (rest, Head::Inherit)
        } else if let Some(rest) = before.strip_suffix('*') {
            (rest, Head::Composition)
        } else if let Some(rest) = before.strip_suffix('<') {
            (rest, Head::Arrow)
        } else if before.ends_with(" o") {
            (&before[..before.len() - 1], Head::Aggregation)
        } else {
            (before, Head::None)
        };
        let (after, to_end) = if let Some(rest) = after.strip_prefix("|>") {
            (rest, Head::Inherit)
        } else if let Some(rest) = after.strip_prefix('*') {
            (rest, Head::Composition)
        } else if let Some(rest) = after.strip_prefix('>') {
            (rest, Head::Arrow)
        } else if after.starts_with("o ") {
            (&after[1..], Head::Aggregation)
        } else {
            (after, Head::None)
        };
        let (right, label) = match after.split_once(':') {
            Some((right, label)) => (right, Some(label.trim().to_string())),
            None => (after, None),
        };
        let from = self.class(without_cardinality(before, true))?;
        let to = self.class(without_cardinality(right, false))?;
        self.b.flow.edges.push(Edge {
            from,
            to,
            from_end,
            to_end,
            weight: if dotted {
                Weight::Dotted
            } else {
                Weight::Normal
            },
            label: label.filter(|l| !l.is_empty()),
        });
        Some(())
    }
}

/// The class name of a relation end, without the quoted cardinality next to the line.
fn without_cardinality(end: &str, trailing: bool) -> &str {
    let end = end.trim();
    if trailing && end.ends_with('"') {
        let body = &end[..end.len() - 1];
        return body.rfind('"').map_or(end, |at| body[..at].trim());
    }
    if !trailing && let Some(body) = end.strip_prefix('"') {
        return body.split_once('"').map_or(end, |(_, rest)| rest.trim());
    }
    end
}

/// Parses a `classDiagram` block. `None` for another diagram type and for syntax that is not
/// drawn: namespaces, notes and links.
pub fn parse(src: &str) -> Option<Flow> {
    let mut lines = src
        .lines()
        .map(|l| l.trim().trim_end_matches(';').trim_end())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    if !matches!(lines.next()?, "classDiagram" | "classDiagram-v2") {
        return None;
    }
    let mut parser = Parser {
        b: Builder::new(Direction::TopDown),
        members: HashMap::new(),
    };
    let mut open: Option<usize> = None;
    for line in lines {
        if let Some(index) = open {
            if line == "}" {
                open = None;
            } else {
                parser.add_member(index, line);
            }
            continue;
        }
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let relation = ["--", ".."]
            .iter()
            .filter_map(|op| line.find(op).map(|at| (at, *op == "..")))
            .min_by_key(|&(at, _)| at);
        let colon = line.find(':');
        match word {
            "class" => {
                let rest = rest.trim();
                match rest.strip_suffix('{') {
                    Some(name) => open = Some(parser.class(name.trim())?),
                    None => {
                        parser.class(rest)?;
                    }
                }
            }
            "direction" => parser.b.flow.direction = Direction::parse(rest.trim())?,
            "classDef" | "cssClass" | "style" | "accTitle" | "accDescr" => {}
            "namespace" | "note" | "link" | "click" | "callback" => return None,
            _ => match (relation, colon) {
                (Some((at, dotted)), colon) if colon.is_none_or(|c| c > at) => {
                    parser.relation(line, at, dotted)?;
                }
                (_, Some(colon)) => {
                    let index = parser.class(line[..colon].trim())?;
                    parser.add_member(index, &line[colon + 1..]);
                }
                _ => return None,
            },
        }
    }
    if open.is_some() {
        return None;
    }
    let mut flow = parser.b.flow;
    for (index, members) in parser.members {
        let node = &mut flow.nodes[index];
        let mut text = members.annotations;
        text.push(node.lines[0].clone());
        let mut dividers = Vec::new();
        if !members.fields.is_empty() {
            dividers.push(text.len());
            text.extend(members.fields);
        }
        if !members.methods.is_empty() {
            dividers.push(text.len());
            text.extend(members.methods);
        }
        node.lines = text;
        node.dividers = dividers;
    }
    (!flow.nodes.is_empty()).then_some(flow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks_members_and_relations() {
        let src = "classDiagram\n class Animal {\n  +int age\n  +eat() void\n }\n \
                   Animal <|-- Dog\n Dog : +bark()\n Dog *-- Tail : has\n Dog ..> Food\n";
        let flow = parse(src).unwrap();
        assert_eq!(flow.nodes[0].lines, ["Animal", "+int age", "+eat() void"]);
        assert_eq!(flow.nodes[0].dividers, [1, 2]);
        assert_eq!(flow.nodes[1].lines, ["Dog", "+bark()"]);
        let ends: Vec<_> = flow
            .edges
            .iter()
            .map(|e| (e.from_end, e.to_end, e.weight))
            .collect();
        assert_eq!(
            ends,
            [
                (Head::Inherit, Head::None, Weight::Normal),
                (Head::Composition, Head::None, Weight::Normal),
                (Head::None, Head::Arrow, Weight::Dotted),
            ]
        );
        assert_eq!(flow.edges[1].label.as_deref(), Some("has"));
    }

    #[test]
    fn cardinalities_and_generics() {
        let flow = parse("classDiagram\n Order \"1\" --> \"*\" Line\n class Box~T~\n").unwrap();
        assert_eq!(flow.nodes[0].lines, ["Order"]);
        assert_eq!(flow.nodes[1].lines, ["Line"]);
        assert_eq!(flow.nodes[2].lines, ["Box<T>"]);
    }

    #[test]
    fn rejects_unsupported_syntax() {
        assert!(parse("classDiagram\n namespace N {\n }\n").is_none());
        assert!(parse("classDiagram\n class A {\n +x\n").is_none());
        assert!(parse("stateDiagram\n A --> B").is_none());
    }
}
