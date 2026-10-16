//! State diagrams: parsed into the graph drawn by the flowchart layout.

use super::canvas::Weight;
use super::graph::{Builder, Direction, Edge, Flow, Head, Shape};

/// The pseudo-state name used for the initial and the final state.
const PSEUDO: &str = "[*]";

struct Parser {
    b: Builder,
    start: Option<usize>,
    end: Option<usize>,
}

impl Parser {
    fn state(&mut self, id: &str) -> Option<usize> {
        if id.is_empty() || id.contains(char::is_whitespace) {
            return None;
        }
        let index = self.b.node(id);
        self.b.flow.nodes[index].shape = Shape::Round;
        Some(index)
    }

    /// Source of a transition: `[*]` is the initial state.
    fn source(&mut self, id: &str) -> Option<usize> {
        if id != PSEUDO {
            return self.state(id);
        }
        Some(
            *self
                .start
                .get_or_insert_with(|| self.b.anonymous(Shape::Start)),
        )
    }

    /// Target of a transition: `[*]` is the final state.
    fn target(&mut self, id: &str) -> Option<usize> {
        if id != PSEUDO {
            return self.state(id);
        }
        Some(*self.end.get_or_insert_with(|| self.b.anonymous(Shape::End)))
    }

    fn transition(&mut self, from: &str, rest: &str) -> Option<()> {
        let (to, label) = match rest.split_once(':') {
            Some((to, label)) => (to.trim(), Some(label.trim().to_string())),
            None => (rest.trim(), None),
        };
        let (from, to) = (self.source(from.trim())?, self.target(to)?);
        self.b.flow.edges.push(Edge {
            from,
            to,
            from_end: Head::None,
            to_end: Head::Arrow,
            weight: Weight::Normal,
            label: label.filter(|l| !l.is_empty()),
        });
        Some(())
    }

    /// `id : text` adds a description line under the state name.
    fn describe(&mut self, id: &str, text: &str) -> Option<()> {
        let index = self.state(id.trim())?;
        let node = &mut self.b.flow.nodes[index];
        if node.dividers.is_empty() {
            node.dividers.push(1);
        }
        node.lines.push(text.trim().to_string());
        Some(())
    }

    /// `state "Long name" as id` replaces the name shown.
    fn rename(&mut self, rest: &str) -> Option<()> {
        let quoted = rest.strip_prefix('"')?;
        let (name, after) = quoted.split_once('"')?;
        let index = self.state(after.trim().strip_prefix("as ")?.trim())?;
        self.b.flow.nodes[index].lines[0] = name.to_string();
        Some(())
    }
}

/// Parses a `stateDiagram` block. `None` for another diagram type and for syntax that is not
/// drawn: composite states, forks, choices, notes and concurrency.
pub fn parse(src: &str) -> Option<Flow> {
    let mut lines = src
        .lines()
        .map(|l| l.trim().trim_end_matches(';').trim_end())
        .filter(|l| !l.is_empty() && !l.starts_with("%%"));
    if !matches!(lines.next()?, "stateDiagram" | "stateDiagram-v2") {
        return None;
    }
    let mut parser = Parser {
        b: Builder::new(Direction::TopDown),
        start: None,
        end: None,
    };
    for line in lines {
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        if let Some((from, to)) = line.split_once("-->") {
            parser.transition(from, to)?;
        } else if word == "direction" {
            parser.b.flow.direction = Direction::parse(rest.trim())?;
        } else if word == "state" {
            parser.rename(rest.trim())?;
        } else if matches!(
            word,
            "classDef" | "class" | "style" | "accTitle" | "accDescr"
        ) {
            continue;
        } else if let Some((id, text)) = line.split_once(':') {
            parser.describe(id, text)?;
        } else {
            return None;
        }
    }
    (!parser.b.flow.nodes.is_empty()).then_some(parser.b.flow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_transitions_and_pseudo_states() {
        let flow =
            parse("stateDiagram-v2\n [*] --> Idle\n Idle --> Busy : go\n Busy --> [*]\n").unwrap();
        assert_eq!(flow.nodes.len(), 4);
        assert_eq!(flow.nodes[0].shape, Shape::Start);
        assert_eq!(flow.nodes[3].shape, Shape::End);
        assert_eq!(flow.edges.len(), 3);
        assert_eq!(flow.edges[1].label.as_deref(), Some("go"));
    }

    #[test]
    fn descriptions_and_names() {
        let flow =
            parse("stateDiagram\n state \"Waiting room\" as W\n W : for the doctor\n").unwrap();
        assert_eq!(flow.nodes[0].lines, ["Waiting room", "for the doctor"]);
        assert_eq!(flow.nodes[0].dividers, [1]);
    }

    #[test]
    fn rejects_unsupported_syntax() {
        assert!(parse("stateDiagram-v2\n state X {\n A --> B\n }\n").is_none());
        assert!(parse("stateDiagram-v2\n state f <<fork>>\n").is_none());
        assert!(parse("stateDiagram-v2\n note right of A: x\n").is_none());
        assert!(parse("graph TD\n A --> B").is_none());
    }
}
