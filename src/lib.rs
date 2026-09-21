//! Markdown parsing and terminal rendering, with the commands built on them.

pub mod ansi;
pub mod cli_extras;
pub mod cmd;
pub mod highlight;
pub mod html;
pub mod layout;
pub mod model;
pub mod options;
pub mod parse;
pub mod style;
pub mod table;
pub mod term;
pub mod wrap;

use std::io::Write;

use anyhow::Result;

pub use options::RenderOptions;

use ansi::AnsiWriter;
use highlight::Highlighter;
use layout::Renderer;
use model::{Block, Document, Segment};
use options::LinkMode;
use style::ColorLevel;

fn has_code(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::CodeBlock { .. } => true,
        Block::BlockQuote { blocks, .. } => has_code(blocks),
        Block::List { items, .. } => items.iter().any(|i| has_code(&i.blocks)),
        Block::FrontMatter { .. } => true,
        _ => false,
    })
}

/// Parses `md` and expands its HTML blocks.
pub fn document(md: &str, opts: &RenderOptions) -> Document {
    let parsed = parse::parse(md, &opts.theme, opts.links);
    Document {
        blocks: html::expand(parsed.blocks, &opts.theme, opts.links),
        footnotes: parsed
            .footnotes
            .into_iter()
            .map(|blocks| html::expand(blocks, &opts.theme, opts.links))
            .collect(),
    }
}

/// Appends the footnote bodies as a numbered list under a rule.
fn footnote_blocks(footnotes: Vec<Vec<Block>>) -> Option<[Block; 2]> {
    if footnotes.is_empty() {
        return None;
    }
    let items = footnotes
        .into_iter()
        .map(|blocks| model::ListItem { task: None, blocks })
        .collect();
    Some([
        Block::Rule,
        Block::List {
            start: Some(1),
            tight: false,
            items,
        },
    ])
}

/// Renders `md` to `out` according to `opts`.
pub fn render(md: &str, opts: &RenderOptions, out: &mut dyn Write) -> Result<()> {
    render_document(document(md, opts), opts, out)
}

/// Renders an already parsed document, as produced by `document`.
pub fn render_document(doc: Document, opts: &RenderOptions, out: &mut dyn Write) -> Result<()> {
    let mut blocks = doc.blocks;
    if let Some(tail) = footnote_blocks(doc.footnotes) {
        blocks.extend(tail);
    }
    let highlighter = if opts.color != ColorLevel::None && has_code(&blocks) {
        Some(Highlighter::new(&opts.code_theme, opts.theme.code_block)?)
    } else {
        None
    };
    let writer = AnsiWriter::new(out, opts.color, opts.links == LinkMode::Osc8);
    let mut renderer = Renderer::new(writer, opts, highlighter.as_ref());
    renderer.render_blocks(&blocks, false)?;
    renderer.finish()?;
    Ok(())
}

/// Writes a file name and a rule, used to separate documents when several are rendered.
pub fn render_file_header(name: &str, opts: &RenderOptions, out: &mut dyn Write) -> Result<()> {
    let mut writer = AnsiWriter::new(out, opts.color, false);
    writer.write_segment(&Segment::new(name, opts.theme.file_header))?;
    writer.end_line()?;
    let rule = opts.glyphs.rule.to_string().repeat(opts.width);
    writer.write_segment(&Segment::new(rule, opts.theme.rule))?;
    writer.end_line()?;
    writer.flush()?;
    Ok(())
}

/// Convenience for tests: renders to a `String`.
pub fn render_to_string(md: &str, opts: &RenderOptions) -> Result<String> {
    let mut buf = Vec::new();
    render(md, opts, &mut buf)?;
    Ok(String::from_utf8(buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use options::FrontMatter;

    #[test]
    fn front_matter_shown_on_request() {
        let mut opts = RenderOptions::plain(40);
        assert_eq!(
            render_to_string("---\ntitle: x\n---\n\nbody\n", &opts).unwrap(),
            "body\n"
        );
        opts.front_matter = FrontMatter::Show;
        assert_eq!(
            render_to_string("---\ntitle: x\n---\n\nbody\n", &opts).unwrap(),
            "  title: x\n\nbody\n"
        );
    }
}
