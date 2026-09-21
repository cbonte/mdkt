//! Shell completion and man page generation, shared by the command line.

use std::io::{self, Write};

use clap::Command;
use clap_complete::Shell;

/// True when the error comes from a reader that closed the pipe, as `head` does.
/// Such an exit is normal and must not be reported.
pub fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|e| e.downcast_ref::<io::Error>())
        .any(|e| e.kind() == io::ErrorKind::BrokenPipe)
}

/// Writes the completion script or the man page to stdout when one was asked for.
/// Returns true when it handled the call and the program should exit.
pub fn handle(cmd: &mut Command, completions: Option<Shell>, man: bool) -> io::Result<bool> {
    let mut out = io::stdout().lock();
    if let Some(shell) = completions {
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell, cmd, name, &mut out);
        out.flush()?;
        return Ok(true);
    }
    if man {
        clap_mangen::Man::new(cmd.clone()).render(&mut out)?;
        let root = cmd.get_name().to_string();
        for sub in cmd.get_subcommands() {
            let name = format!("{root}-{}", sub.get_name());
            clap_mangen::Man::new(sub.clone().name(name)).render(&mut out)?;
        }
        out.flush()?;
        return Ok(true);
    }
    Ok(false)
}
