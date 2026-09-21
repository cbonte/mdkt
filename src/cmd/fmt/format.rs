//! Markdown formatter: rewraps prose and normalises block syntax, without changing markup.

use super::{FmtOptions, emit};

/// Formats `src` according to `opts`.
pub fn format(src: &str, opts: &FmtOptions) -> String {
    let tree = crate::tree::parse(src);
    let out = emit::Emitter::new(src, opts).run(&tree);
    // A formatted document ends with exactly one newline, and an empty one with none.
    let trimmed = out.trim_end_matches('\n');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}
