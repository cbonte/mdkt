use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;

use crate::options::RenderOptions;
use crate::toc::{self, TocOptions, defaults};

/// Options of `mdkt toc`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Markdown files to process; reads stdin when omitted, or for "-".
    files: Vec<PathBuf>,

    /// Shallowest heading level listed.
    #[arg(long, default_value_t = defaults::MIN_DEPTH, value_name = "N")]
    min_depth: u8,

    /// Deepest heading level listed.
    #[arg(long, default_value_t = defaults::MAX_DEPTH, value_name = "N")]
    max_depth: u8,

    /// List marker.
    #[arg(long, default_value_t = defaults::BULLET)]
    bullet: char,

    /// Name used in the HTML comment markers surrounding the table.
    #[arg(long, default_value = defaults::MARKER, value_name = "NAME")]
    marker: String,

    /// Update the files in place instead of writing to stdout.
    #[arg(short, long)]
    write: bool,

    /// Report the files whose table is stale and exit 1; write nothing.
    #[arg(long, conflicts_with = "write")]
    check: bool,
}

impl Args {
    fn toc_options(&self) -> TocOptions {
        TocOptions {
            min_depth: self.min_depth,
            max_depth: self.max_depth,
            bullet: self.bullet,
            marker: self.marker.clone(),
        }
    }
}

fn read_input(path: &Path) -> Result<String> {
    if path.as_os_str() == "-" {
        let mut text = String::new();
        io::stdin()
            .read_to_string(&mut text)
            .context("cannot read stdin")?;
        return Ok(text);
    }
    std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

/// Table of contents of `src`, ignoring the headings already inside the markers.
fn table_of_contents(src: &str, opts: &TocOptions) -> String {
    let body = toc::without_toc(src, opts);
    let doc = crate::document(&body, &RenderOptions::default());
    toc::generate(&doc, opts)
}

/// Returns true when the file was changed or would be.
fn process(cli: &Args, path: &Path, out: &mut dyn Write) -> Result<bool> {
    let src = read_input(path)?;
    let opts = cli.toc_options();
    let table = table_of_contents(&src, &opts);
    let Some(updated) = toc::insert(&src, &table, &opts) else {
        if cli.write || cli.check {
            bail!(
                "{} has no {} and {} markers",
                path.display(),
                opts.open_marker(),
                opts.close_marker()
            );
        }
        // Without markers there is nothing to update, so the table alone is useful.
        out.write_all(table.as_bytes())?;
        return Ok(false);
    };
    let changed = updated != src;
    if cli.check {
        if changed {
            writeln!(out, "{}", path.display())?;
        }
        return Ok(changed);
    }
    if cli.write {
        if changed {
            std::fs::write(path, &updated)
                .with_context(|| format!("cannot write {}", path.display()))?;
        }
        return Ok(changed);
    }
    out.write_all(updated.as_bytes())?;
    Ok(changed)
}

/// Returns the number of files that failed and whether any file changed.
fn update_files(cli: &Args) -> Result<(usize, bool)> {
    let files: Vec<&Path> = if cli.files.is_empty() {
        vec![Path::new("-")]
    } else {
        cli.files.iter().map(|p| p.as_path()).collect()
    };
    if cli.write && files.iter().any(|p| p.as_os_str() == "-") {
        bail!("--write needs a file, not stdin");
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut failures = 0;
    let mut changed = false;
    for path in &files {
        match process(cli, path, &mut out) {
            Ok(c) => changed |= c,
            Err(e) => {
                eprintln!("mdkt toc: {e:#}");
                failures += 1;
            }
        }
    }
    out.flush()?;
    Ok((failures, changed))
}

/// Runs `mdkt toc`; the exit code is a failure when a file failed, or with `--check`
/// when a table is stale.
pub fn run(args: &Args) -> Result<ExitCode> {
    let (failures, changed) = update_files(args)?;
    Ok(if failures == 0 && !(args.check && changed) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
