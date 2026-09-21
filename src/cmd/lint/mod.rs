use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args as ClapArgs, ValueEnum};

pub mod finding;
pub mod rules;

use finding::{DEFAULT_RULES, Rule};

/// Line length checked by the line-length rule when it is enabled.
const DEFAULT_LINE_LENGTH: usize = 100;
/// Directories never walked when an argument is a directory.
const SKIPPED_DIRS: [&str; 3] = [".git", "target", "node_modules"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, ValueEnum)]
enum Format {
    #[default]
    Text,
    Json,
}

/// Options of `mdkt lint`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Markdown files or directories to check; reads stdin when omitted, or for "-".
    paths: Vec<PathBuf>,

    /// Turn a rule off; repeat for several.
    #[arg(long, value_enum, value_name = "RULE")]
    disable: Vec<Rule>,

    /// Turn on a rule that is off by default; repeat for several.
    #[arg(long, value_enum, value_name = "RULE")]
    enable: Vec<Rule>,

    /// Maximum line length for the line-length rule.
    #[arg(long, default_value_t = DEFAULT_LINE_LENGTH, value_name = "N")]
    line_length: usize,

    /// Output format.
    #[arg(long, value_enum, default_value_t)]
    format: Format,

    /// List the rules with their default state and exit.
    #[arg(long)]
    list_rules: bool,
}

impl Args {
    fn enabled(&self) -> HashSet<Rule> {
        let mut set: HashSet<Rule> = DEFAULT_RULES.into_iter().collect();
        set.extend(self.enable.iter().copied());
        for rule in &self.disable {
            set.remove(rule);
        }
        set
    }
}

/// Markdown files under `path`, or the path itself when it is a file.
fn collect(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !path.is_dir() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .with_context(|| format!("cannot read {}", path.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for entry in entries {
        let name = entry.file_name().unwrap_or_default().to_string_lossy();
        if entry.is_dir() {
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_ref()) {
                collect(&entry, out)?;
            }
        } else if entry
            .extension()
            .is_some_and(|e| e == "md" || e == "markdown")
        {
            out.push(entry);
        }
    }
    Ok(())
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

/// Returns the number of findings and the number of files that could not be read.
fn check_files(cli: &Args) -> Result<(usize, usize)> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    if cli.list_rules {
        for rule in Rule::value_variants() {
            let state = if rule.on_by_default() { "on" } else { "off" };
            writeln!(out, "{rule}\t{state}")?;
        }
        out.flush()?;
        return Ok((0, 0));
    }
    let mut files = Vec::new();
    if cli.paths.is_empty() {
        files.push(PathBuf::from("-"));
    } else {
        for path in &cli.paths {
            collect(path, &mut files)?;
        }
    }
    let enabled = cli.enabled();
    let mut findings = 0;
    let mut failures = 0;
    for path in &files {
        let text = match read_input(path) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("mdkt lint: {e:#}");
                failures += 1;
                continue;
            }
        };
        let name = path.display().to_string();
        for finding in rules::check(path, &text, &enabled, cli.line_length) {
            findings += 1;
            match cli.format {
                Format::Text => writeln!(out, "{}", finding.render(&name))?,
                Format::Json => writeln!(out, "{}", finding.render_json(&name))?,
            }
        }
    }
    out.flush()?;
    Ok((findings, failures))
}

/// Runs `mdkt lint`; the exit code is a failure when a rule fired or a file could not be read.
pub fn run(args: &Args) -> Result<ExitCode> {
    Ok(match check_files(args)? {
        (0, 0) => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    })
}
