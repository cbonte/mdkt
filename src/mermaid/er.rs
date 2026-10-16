//! Entity-relationship diagrams: parsed into the graph drawn by the flowchart layout.

use std::collections::HashMap;

use super::canvas::Weight;
use super::graph::{Builder, Direction, Edge, Flow, Head};

struct Parser {
    b: Builder,
    attributes: HashMap<usize, Vec<Vec<String>>>,
}

/// Name of an entity: a word, or a quoted text.
fn entity_name(token: &str) -> Option<&str> {
    let name = token
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(token);
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    (!name.is_empty() && (token.len() != name.len() || name.chars().all(word))).then_some(name)
}

/// Cardinality written next to an entity, as `..` of the minimum and the maximum.
fn cardinality(token: &str) -> Option<&'static str> {
    match token {
        "||" => Some("1"),
        "|o" | "o|" => Some("0..1"),
        "}o" | "o{" => Some("0..*"),
        "}|" | "|{" => Some("1..*"),
        _ => None,
    }
}

impl Parser {
    fn entity(&mut self, token: &str) -> Option<usize> {
        Some(self.b.node(entity_name(token)?))
    }

    /// `A ||--o{ B : places`
    fn relation(&mut self, line: &str) -> Option<()> {
        let (head, label) = match line.split_once(':') {
            Some((head, label)) => (head, label.trim().trim_matches('"').trim()),
            None => (line, ""),
        };
        let mut tokens = head.split_whitespace();
        let (a, rel, b) = (tokens.next()?, tokens.next()?, tokens.next()?);
        if tokens.next().is_some() || !rel.is_ascii() || rel.len() != 6 {
            return None;
        }
        let dotted = match &rel[2..4] {
            "--" => false,
            ".." => true,
            _ => return None,
        };
        let (left, right) = (cardinality(&rel[..2])?, cardinality(&rel[4..])?);
        let (from, to) = (self.entity(a)?, self.entity(b)?);
        let counts = format!("({left} to {right})");
        self.b.flow.edges.push(Edge {
            from,
            to,
            from_end: Head::None,
            to_end: Head::None,
            weight: if dotted {
                Weight::Dotted
            } else {
                Weight::Normal
            },
            label: Some(if label.is_empty() {
                counts
            } else {
                format!("{label} {counts}")
            }),
        });
        Some(())
    }

    /// `string name PK "comment"`: the type, the name and the keys.
    fn attribute(&mut self, index: usize, line: &str) -> Option<()> {
        let line = line.split('"').next().unwrap_or("");
        let mut words = line.split_whitespace();
        let (kind, name) = (words.next()?, words.next()?);
        let keys: Vec<&str> = words.collect();
        self.attributes.entry(index).or_default().push(vec![
            kind.to_string(),
            name.to_string(),
            keys.join(","),
        ]);
        Some(())
    }
}

/// Parses an `erDiagram` block. `None` for another diagram type and for syntax that is not drawn.
pub fn parse(src: &str) -> Option<Flow> {
    let mut lines = src
        .lines()
        .map(|l| l.trim().trim_end_matches(';').trim_end())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    if lines.next()? != "erDiagram" {
        return None;
    }
    let mut parser = Parser {
        b: Builder::new(Direction::TopDown),
        attributes: HashMap::new(),
    };
    let mut open: Option<usize> = None;
    for line in lines {
        if let Some(index) = open {
            if line == "}" {
                open = None;
            } else {
                parser.attribute(index, line)?;
            }
            continue;
        }
        let word = line.split_whitespace().next().unwrap_or("");
        if matches!(word, "title" | "accTitle" | "accDescr") {
            continue;
        }
        if let Some(name) = line.strip_suffix('{') {
            open = Some(parser.entity(name.trim())?);
        } else if line.split_whitespace().count() == 1 {
            parser.entity(line)?;
        } else {
            parser.relation(line)?;
        }
    }
    if open.is_some() {
        return None;
    }
    let mut flow = parser.b.flow;
    for (index, rows) in parser.attributes {
        let widths: Vec<usize> = (0..3)
            .map(|c| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0))
            .collect();
        let node = &mut flow.nodes[index];
        node.dividers = vec![1];
        node.lines.extend(rows.iter().map(|r| {
            let line = format!(
                "{:<w0$} {:<w1$} {}",
                r[0],
                r[1],
                r[2],
                w0 = widths[0],
                w1 = widths[1]
            );
            line.trim_end().to_string()
        }));
    }
    (!flow.nodes.is_empty()).then_some(flow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entities_attributes_and_relations() {
        let src = "erDiagram\n CUSTOMER ||--o{ ORDER : places\n CUSTOMER {\n  string name\n  \
                   int id PK\n }\n ORDER }|..|| \"LINE-ITEM\" : \"has\"\n";
        let flow = parse(src).unwrap();
        assert_eq!(
            flow.nodes[0].lines,
            ["CUSTOMER", "string name", "int    id   PK"]
        );
        assert_eq!(flow.nodes[0].dividers, [1]);
        assert_eq!(flow.nodes[2].lines, ["LINE-ITEM"]);
        assert_eq!(flow.edges[0].label.as_deref(), Some("places (1 to 0..*)"));
        assert_eq!(flow.edges[1].weight, Weight::Dotted);
        assert_eq!(flow.edges[1].label.as_deref(), Some("has (1..* to 1)"));
    }

    #[test]
    fn rejects_unsupported_syntax() {
        assert!(parse("erDiagram\n A ||--o{ B\n A <-> B : x\n").is_none());
        assert!(parse("erDiagram\n A {\n int x\n").is_none());
        assert!(parse("classDiagram\n A --> B").is_none());
    }
}
