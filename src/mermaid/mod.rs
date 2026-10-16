//! Mermaid diagrams drawn as text.

mod canvas;
mod flow;
mod route;

use crate::model::Line;
use crate::options::RenderOptions;

/// Draws `code` within `width` columns. `None` when the diagram type or syntax is not drawn, or
/// when the drawing does not fit.
pub fn render(code: &str, width: usize, opts: &RenderOptions) -> Option<Vec<Line>> {
    let flow = flow::parse(code)?;
    flow::draw(&flow, width, opts)
}
