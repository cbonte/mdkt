//! Spawning a pager and writing the rendered document into it.

use std::io;
use std::process::{Child, Command, Stdio};

/// Pager used when neither the environment nor the caller names one.
const DEFAULT_PAGER: &str = "less";
/// Arguments given to the default pager so it keeps escape sequences and exits on a short file.
const DEFAULT_PAGER_ARGS: [&str; 2] = ["-R", "-F"];
/// Flags that already tell less to pass escape sequences through.
const RAW_FLAGS: [&str; 4] = ["-R", "-r", "--RAW-CONTROL-CHARS", "--raw-control-chars"];

/// Program and arguments for `value`, adding a raw flag when `less` would otherwise
/// print escape sequences literally. An empty or blank value selects the default pager.
pub fn command_line(value: Option<&str>) -> (String, Vec<String>) {
    let value = value.unwrap_or("").trim();
    if value.is_empty() {
        return (
            DEFAULT_PAGER.to_string(),
            DEFAULT_PAGER_ARGS.iter().map(|a| a.to_string()).collect(),
        );
    }
    let mut parts = value.split_whitespace().map(str::to_string);
    let program = parts.next().expect("non-empty after trim");
    let mut args: Vec<String> = parts.collect();
    let basename = program.rsplit('/').next().unwrap_or(&program);
    if basename == "less" && !args.iter().any(|a| RAW_FLAGS.contains(&a.as_str())) {
        args.push("-R".to_string());
    }
    (program, args)
}

/// Value of the first pager variable that is set, in order of precedence.
pub fn from_env() -> Option<String> {
    ["MDKT_PAGER", "PAGER"]
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .filter(|v| !v.trim().is_empty())
}

/// Starts the pager with its stdin piped and its output inherited.
pub fn spawn(value: Option<&str>) -> io::Result<Child> {
    let (program, args) = command_line(value);
    Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pager_keeps_escapes() {
        assert_eq!(
            command_line(None),
            ("less".into(), vec!["-R".to_string(), "-F".to_string()])
        );
        assert_eq!(command_line(Some("   ")), command_line(None));
    }

    #[test]
    fn raw_flag_added_to_less_only_once() {
        assert_eq!(
            command_line(Some("less -S")),
            ("less".into(), vec!["-S".to_string(), "-R".to_string()])
        );
        assert_eq!(
            command_line(Some("/usr/bin/less -R -S")),
            (
                "/usr/bin/less".into(),
                vec!["-R".to_string(), "-S".to_string()]
            )
        );
        assert_eq!(
            command_line(Some("less --RAW-CONTROL-CHARS")),
            ("less".into(), vec!["--RAW-CONTROL-CHARS".to_string()])
        );
    }

    #[test]
    fn other_pagers_are_left_alone() {
        assert_eq!(command_line(Some("more")), ("more".into(), Vec::new()));
        assert_eq!(
            command_line(Some("bat --style plain")),
            (
                "bat".into(),
                vec!["--style".to_string(), "plain".to_string()]
            )
        );
    }
}
