//! Mermaid diagrams drawn as text.

mod canvas;
mod class;
mod er;
mod flow;
mod graph;
mod parse;
mod pie;
mod route;
mod sequence;
mod state;

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
    if first.split_whitespace().next() == Some("pie") {
        return pie::render(code, width, opts);
    }
    let flow = match first.split_whitespace().next() {
        Some("stateDiagram" | "stateDiagram-v2") => state::parse(code)?,
        Some("classDiagram" | "classDiagram-v2") => class::parse(code)?,
        Some("erDiagram") => er::parse(code)?,
        _ => parse::parse(code)?,
    };
    flow::draw(&flow, width, opts)
}
