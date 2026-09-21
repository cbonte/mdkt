//! Helpers shared by the command line.

use std::io;

/// True when the error comes from a reader that closed the pipe, as `head` does.
/// Such an exit is normal and must not be reported.
pub fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|e| e.downcast_ref::<io::Error>())
        .any(|e| e.kind() == io::ErrorKind::BrokenPipe)
}
