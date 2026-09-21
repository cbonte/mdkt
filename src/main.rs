use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use mdkt::cli_extras::{self, is_broken_pipe};
use mdkt::cmd::{cat, lint, toc};

/// Markdown tools for the terminal.
#[derive(Parser, Debug)]
#[command(name = "mdkt", version, about, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Print a completion script for the given shell and exit.
    #[arg(long, value_enum, value_name = "SHELL", hide_short_help = true)]
    completions: Option<Shell>,

    /// Print the manual, one roff page per command, and exit.
    #[arg(long, hide_short_help = true)]
    man: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Render Markdown as styled text in the terminal.
    Cat(cat::Args),
    /// Insert or refresh a table of contents in a Markdown file.
    Toc(toc::Args),
    /// Check Markdown links, anchors and heading structure.
    Lint(lint::Args),
}

fn dispatch(command: Command) -> (&'static str, anyhow::Result<ExitCode>) {
    match command {
        Command::Cat(args) => ("cat", cat::run(&args)),
        Command::Toc(args) => ("toc", toc::run(&args)),
        Command::Lint(args) => ("lint", lint::run(&args)),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli_extras::handle(&mut Cli::command(), cli.completions, cli.man) {
        Ok(true) => return ExitCode::SUCCESS,
        Ok(false) => {}
        Err(e) => {
            eprintln!("mdkt: {e}");
            return ExitCode::FAILURE;
        }
    }
    let Some(command) = cli.command else {
        return ExitCode::SUCCESS;
    };
    let (name, result) = dispatch(command);
    match result {
        Ok(code) => code,
        // A reader that closed the pipe, as head does, is not a failure.
        Err(e) if is_broken_pipe(&e) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mdkt {name}: {e:#}");
            ExitCode::FAILURE
        }
    }
}
