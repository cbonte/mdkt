use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};

use crate::RenderOptions;
use crate::cli_extras::is_broken_pipe;
use crate::pager;
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

fn render_one(
    cli: &Args,
    path: &Path,
    header: bool,
    opts: &RenderOptions,
    out: &mut dyn Write,
) -> Result<()> {
    let text = read_input(path)?;
    if header {
        crate::render_file_header(&path.display().to_string(), opts, out)?;
    }
    let doc = crate::document(&text, opts);
    if cli.toc {
        return crate::render_outline(&doc, cli.toc_depth, opts, out);
    }
    let doc = match &cli.section {
        Some(name) => crate::section(&doc, name)
            .with_context(|| format!("no heading matching '{name}' in {}", path.display()))?,
        None => doc,
    };
    crate::render_document(doc, opts, out)
}

/// Renders every file into `out`; returns the number of files that failed.
fn render_all(
    cli: &Args,
    files: &[&Path],
    header: bool,
    opts: &RenderOptions,
    out: &mut dyn Write,
) -> Result<usize> {
    let mut failures = 0;
    for (i, path) in files.iter().enumerate() {
        if i > 0 {
            out.write_all(b"\n")?;
        }
        if let Err(e) = render_one(cli, path, header, opts, out) {
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

/// Renders every file; returns the number of files that failed.
fn render_files(cli: &Args) -> Result<usize> {
    if cli.list_themes {
        let mut out = io::stdout().lock();
        for name in crate::highlight::theme_names() {
            writeln!(out, "{name}")?;
        }
        return Ok(0);
    }
    let probe = Probe::from_env();
    let opts = resolve(cli, &probe);
    let files: Vec<&Path> = if cli.files.is_empty() {
        vec![Path::new("-")]
    } else {
        cli.files.iter().map(|p| p.as_path()).collect()
    };
    let header = files.len() > 1 && !cli.no_header;
    let child = if cli.pager && probe.stderr_is_tty {
        match pager::spawn(pager::from_env().as_deref()) {
            Ok(child) => Some(child),
            Err(e) => {
                eprintln!("mdkt cat: cannot start the pager: {e}");
                None
            }
        }
    } else {
        None
    };
    let Some(mut child) = child else {
        let mut out = io::BufWriter::new(io::stdout().lock());
        return render_all(cli, &files, header, &opts, &mut out);
    };
    let mut pipe = io::BufWriter::new(child.stdin.take().expect("piped stdin"));
    let failures = render_all(cli, &files, header, &opts, &mut pipe);
    drop(pipe);
    child.wait()?;
    // The reader quitting early is not a failure.
    match failures {
        Err(e) if is_broken_pipe(&e) => Ok(0),
        other => other,
    }
}

/// Runs `mdkt cat`; the exit code is a failure when a file could not be rendered.
pub fn run(args: &Args) -> Result<ExitCode> {
    Ok(match render_files(args)? {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    })
}
