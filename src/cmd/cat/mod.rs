use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};

use crate::RenderOptions;
use crate::cli_extras::is_broken_pipe;
use crate::term::Probe;

mod args;

pub use args::Args;
use args::resolve;

fn read_input(path: &Path) -> Result<String> {
    let mut text = String::new();
    if path.as_os_str() == "-" {
        io::stdin()
            .read_to_string(&mut text)
            .context("cannot read stdin")?;
    } else {
        text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
    }
    Ok(text)
}

fn render_one(path: &Path, header: bool, opts: &RenderOptions, out: &mut dyn Write) -> Result<()> {
    let text = read_input(path)?;
    if header {
        crate::render_file_header(&path.display().to_string(), opts, out)?;
    }
    crate::render(&text, opts, out)
}

/// Renders every file; returns the number of files that failed.
fn render_files(cli: &Args) -> Result<usize> {
    if cli.list_themes {
        let mut out = io::stdout().lock();
        for name in crate::highlight::theme_names() {
            writeln!(out, "{name}")?;
        }
        return Ok(0);
    }
    let opts = resolve(cli, &Probe::from_env());
    let files: Vec<&Path> = if cli.files.is_empty() {
        vec![Path::new("-")]
    } else {
        cli.files.iter().map(|p| p.as_path()).collect()
    };
    let header = files.len() > 1 && !cli.no_header;
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut failures = 0;
    for (i, path) in files.iter().enumerate() {
        if i > 0 {
            out.write_all(b"\n")?;
        }
        if let Err(e) = render_one(path, header, &opts, &mut out) {
            if is_broken_pipe(&e) {
                return Err(e);
            }
            out.flush()?;
            eprintln!("mdkt cat: {e:#}");
            failures += 1;
        }
    }
    out.flush()?;
    Ok(failures)
}

/// Runs `mdkt cat`; the exit code is a failure when a file could not be rendered.
pub fn run(args: &Args) -> Result<ExitCode> {
    Ok(match render_files(args)? {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    })
}
