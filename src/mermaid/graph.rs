//! Graph shared by the diagrams drawn with the flowchart layout.

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
    /// Initial pseudo-state of a state diagram.
    Start,
    /// Final pseudo-state of a state diagram.
    End,
}

/// End of an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    None,
    Arrow,
    /// Hollow triangle of a class inheritance.
    Inherit,
    /// Filled diamond of a composition.
    Composition,
    /// Hollow diamond of an aggregation.
    Aggregation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Node {
    pub lines: Vec<String>,
    /// Indices in `lines` before which a divider row is drawn.
    pub dividers: Vec<usize>,
    pub shape: Shape,
}

/// An edge laid out from `from` to `to`. The ends may carry a head each.
#[derive(Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub from_end: Head,
    pub to_end: Head,
    pub weight: Weight,
    pub label: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Flow {
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Builds a `Flow`, creating a node on its first mention.
pub struct Builder {
    pub flow: Flow,
    ids: HashMap<String, usize>,
}

impl Builder {
    pub fn new(direction: Direction) -> Builder {
        Builder {
            flow: Flow {
                direction,
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            ids: HashMap::new(),
        }
    }

    /// Index of the node named `id`, created as a rectangle labelled `id` when new.
    pub fn node(&mut self, id: &str) -> usize {
        *self.ids.entry(id.to_string()).or_insert_with(|| {
            self.flow.nodes.push(Node {
                lines: vec![id.to_string()],
                dividers: Vec::new(),
                shape: Shape::Rect,
            });
            self.flow.nodes.len() - 1
        })
    }

    /// A node that is not looked up by name.
    pub fn anonymous(&mut self, shape: Shape) -> usize {
        self.flow.nodes.push(Node {
            lines: Vec::new(),
            dividers: Vec::new(),
            shape,
        });
        self.flow.nodes.len() - 1
    }
}
