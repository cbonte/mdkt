use std::process::ExitCode;

use clap::{Parser, Subcommand};

use mdkt::cli_extras::is_broken_pipe;
use mdkt::cmd::cat;

/// Markdown tools for the terminal.
#[derive(Parser, Debug)]
#[command(name = "mdkt", version, about, arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Render Markdown as styled text in the terminal.
    Cat(cat::Args),
}

fn dispatch(command: Command) -> (&'static str, anyhow::Result<ExitCode>) {
    match command {
        Command::Cat(args) => ("cat", cat::run(&args)),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
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
