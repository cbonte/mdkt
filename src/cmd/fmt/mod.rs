use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;

pub mod emit;
mod format;
pub mod hazards;
pub mod inline;
pub mod options;
pub mod table;
pub mod wrap;

pub use format::format;
pub use options::FmtOptions;

use options::defaults;

/// Options of `mdkt fmt`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Markdown files to format; reads stdin when omitted, or for "-".
    files: Vec<PathBuf>,

    /// Target line width; 0 puts each paragraph on one line.
    #[arg(short, long, default_value_t = defaults::WIDTH, value_name = "N")]
    width: usize,

    /// Marker for unordered lists.
    #[arg(long, default_value_t = defaults::BULLET)]
    bullet: char,

    /// Rewrite the files in place instead of writing to stdout.
    #[arg(short = 'i', long)]
    write: bool,

    /// Report the files that would change and exit 1; write nothing.
    #[arg(long, conflicts_with_all = ["write", "diff"])]
    check: bool,

    /// Print a line diff of the changes instead of the formatted text.
    #[arg(long, conflicts_with = "write")]
    diff: bool,
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

/// Writes a line diff of `before` against `after`, in the unified style but without
/// hunk headers: the whole file is walked once.
fn write_diff(name: &str, before: &str, after: &str, out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "--- {name}")?;
    writeln!(out, "+++ {name}")?;
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let common_head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail_limit = old.len().min(new.len()) - common_head;
    let common_tail = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(tail_limit);
    for line in &old[common_head..old.len() - common_tail] {
        writeln!(out, "-{line}")?;
    }
    for line in &new[common_head..new.len() - common_tail] {
        writeln!(out, "+{line}")?;
    }
    Ok(())
}

/// Returns true when the file changed or would change.
fn process(cli: &Args, path: &Path, opts: &FmtOptions, out: &mut dyn Write) -> Result<bool> {
    let src = read_input(path)?;
    let formatted = format(&src, opts);
    let changed = formatted != src;
    let name = path.display().to_string();
    if cli.check {
        if changed {
            writeln!(out, "{name}")?;
        }
        return Ok(changed);
    }
    if cli.diff {
        if changed {
            write_diff(&name, &src, &formatted, out)?;
        }
        return Ok(changed);
    }
    if cli.write {
        if changed {
            std::fs::write(path, &formatted).with_context(|| format!("cannot write {name}"))?;
        }
        return Ok(changed);
    }
    out.write_all(formatted.as_bytes())?;
    Ok(changed)
}

/// Returns the number of files that failed and whether any file changed.
fn format_files(cli: &Args) -> Result<(usize, bool)> {
    let files: Vec<&Path> = if cli.files.is_empty() {
        vec![Path::new("-")]
    } else {
        cli.files.iter().map(|p| p.as_path()).collect()
    };
    if cli.write && files.iter().any(|p| p.as_os_str() == "-") {
        bail!("--write needs a file, not stdin");
    }
    let opts = FmtOptions {
        bullet: cli.bullet,
        ..FmtOptions::with_width(cli.width)
    };
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut failures = 0;
    let mut changed = false;
    for path in &files {
        match process(cli, path, &opts, &mut out) {
            Ok(c) => changed |= c,
            Err(e) => {
                eprintln!("mdkt fmt: {e:#}");
                failures += 1;
            }
        }
    }
    out.flush()?;
    Ok((failures, changed))
}

/// Runs `mdkt fmt`; the exit code is a failure when a file failed, or with `--check`
/// when a file would change.
pub fn run(args: &Args) -> Result<ExitCode> {
    let (failures, changed) = format_files(args)?;
    Ok(if failures == 0 && !(args.check && changed) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
