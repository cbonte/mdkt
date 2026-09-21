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
use model::{Block, Segment};
use options::LinkMode;
use style::ColorLevel;

fn has_code(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::CodeBlock { .. } => true,
        Block::BlockQuote(inner) => has_code(inner),
        Block::List { items, .. } => items.iter().any(|i| has_code(&i.blocks)),
        _ => false,
    })
}

/// Renders `md` to `out` according to `opts`.
pub fn render(md: &str, opts: &RenderOptions, out: &mut dyn Write) -> Result<()> {
    let blocks = parse::parse(md, &opts.theme, opts.links);
    let blocks = html::expand(blocks, &opts.theme, opts.links);
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
