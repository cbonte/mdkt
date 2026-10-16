//! Mermaid diagrams drawn as text.

mod canvas;
mod flow;
mod parse;
mod route;
mod sequence;

use crate::model::Line;
use crate::options::RenderOptions;

/// Draws `code` within `width` columns. `None` when the diagram type or syntax is not drawn, or
/// when the drawing does not fit.
pub fn render(code: &str, width: usize, opts: &RenderOptions) -> Option<Vec<Line>> {
    let first = code
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))?;
    if first.trim_end_matches(';') == "sequenceDiagram" {
        return sequence::render(code, width, opts);
    }
    let flow = parse::parse(code)?;
    flow::draw(&flow, width, opts)
}
