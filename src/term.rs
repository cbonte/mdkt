//! Terminal capability detection from the environment.

use std::io::IsTerminal;

use crate::style::ColorLevel;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub is_tty: bool,
    /// stdout is a pipe, as when feeding a pager.
    pub stdout_is_pipe: bool,
    /// stderr is a terminal: the process runs in an interactive session.
    pub stderr_is_tty: bool,
    pub width: Option<usize>,
    pub color: ColorLevel,
    pub no_color: bool,
    pub utf8: bool,
    pub osc8: bool,
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn env_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty())
}

#[cfg(unix)]
fn stdout_is_pipe() -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata("/dev/stdout").is_ok_and(|m| m.file_type().is_fifo())
}

#[cfg(not(unix))]
fn stdout_is_pipe() -> bool {
    false
}

/// Programs known to draw 24-bit colour and OSC 8 links.
const RICH_TERM_PROGRAMS: [&str; 5] = ["iTerm.app", "WezTerm", "vscode", "ghostty", "Hyper"];
/// First VTE version with OSC 8 support (0.50.0).
const VTE_OSC8_MIN: u32 = 5000;

impl Probe {
    pub fn from_env() -> Probe {
        let is_tty = std::io::stdout().is_terminal();
        let width = terminal_size::terminal_size()
            .or_else(|| terminal_size::terminal_size_of(std::io::stderr()))
            .map(|(w, _)| w.0 as usize)
            .filter(|w| *w > 0);
        let term = env("TERM");
        let colorterm = env("COLORTERM");
        let program = env("TERM_PROGRAM");
        let rich_program = RICH_TERM_PROGRAMS.contains(&program.as_str());
        let color = if term == "dumb" {
            ColorLevel::None
        } else if colorterm == "truecolor" || colorterm == "24bit" || rich_program {
            ColorLevel::TrueColor
        } else if term.contains("256color") {
            ColorLevel::Ansi256
        } else {
            ColorLevel::Ansi16
        };
        let vte_ok = env("VTE_VERSION")
            .parse::<u32>()
            .is_ok_and(|v| v >= VTE_OSC8_MIN);
        let osc8 = rich_program
            || vte_ok
            || env_set("KITTY_WINDOW_ID")
            || env_set("WT_SESSION")
            || env_set("KONSOLE_VERSION")
            || term.starts_with("xterm-kitty")
            || term.starts_with("foot")
            || term.starts_with("contour");
        let locale = [env("LC_ALL"), env("LC_CTYPE"), env("LANG")]
            .into_iter()
            .find(|v| !v.is_empty())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let utf8 = locale.contains("utf-8") || locale.contains("utf8");
        Probe {
            is_tty,
            stdout_is_pipe: stdout_is_pipe(),
            stderr_is_tty: std::io::stderr().is_terminal(),
            width,
            color,
            no_color: env_set("NO_COLOR"),
            utf8,
            osc8,
        }
    }
}
