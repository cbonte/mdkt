//! End-to-end tests of `mdkt lint`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn run_in(dir: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdkt"))
        .arg("lint")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mdkt lint");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait")
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).expect("utf-8 output")
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("mdkt-lint-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, content).expect("write file");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn clean_document_exits_zero() {
    let dir = TempDir::new("clean");
    dir.file(
        "ok.md",
        "# Title\n\n## Part\n\n[go](#part) and ![alt](img.png)\n",
    );
    dir.file("img.png", "");
    let o = run_in(&dir.0, &["ok.md"], "");
    assert!(o.status.success(), "{}", stdout(&o));
    assert!(stdout(&o).is_empty());
}

#[test]
fn missing_file_and_anchor_are_reported() {
    let dir = TempDir::new("targets");
    dir.file(
        "bad.md",
        "# Title\n\n[gone](missing.md) [anchor](#nope) [ok](#title)\n",
    );
    let o = run_in(&dir.0, &["bad.md"], "");
    assert!(!o.status.success());
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        [
            "bad.md:3:1: link-file: missing.md does not exist",
            "bad.md:3:20: link-anchor: no heading 'nope' in this file",
        ]
    );
}

#[test]
fn anchors_are_checked_across_files() {
    let dir = TempDir::new("cross");
    dir.file("other.md", "# Other\n\n## Deep Part\n");
    dir.file(
        "main.md",
        "# Main\n\n[a](other.md#deep-part) [b](other.md#absent)\n",
    );
    let o = run_in(&dir.0, &["main.md"], "");
    let out = stdout(&o);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        ["main.md:3:25: link-anchor: no heading 'absent' in other.md"]
    );
}

#[test]
fn directories_are_walked_and_skips_apply() {
    let dir = TempDir::new("walk");
    dir.file("a.md", "# A\n\n### Skip\n");
    dir.file("sub/b.md", "# B\n\n### Skip\n");
    dir.file("target/ignored.md", "# C\n\n### Skip\n");
    dir.file("notes.txt", "not markdown");
    let o = run_in(&dir.0, &["."], "");
    let out = stdout(&o);
    assert!(out.contains("a.md:3:1: heading-skip"), "{out}");
    assert!(out.contains("b.md:3:1: heading-skip"), "{out}");
    assert!(!out.contains("ignored.md"), "{out}");
}

#[test]
fn rules_can_be_turned_off_and_on() {
    let dir = TempDir::new("rules");
    dir.file("x.md", "# A\n\n### Skip\n\n```\ncode\n```\n");
    let o = run_in(&dir.0, &["--disable", "heading-skip", "x.md"], "");
    assert!(o.status.success(), "{}", stdout(&o));
    let o = run_in(&dir.0, &["--enable", "fence-lang", "x.md"], "");
    let out = stdout(&o);
    assert!(out.contains("fence-lang"), "{out}");
    assert!(out.contains("heading-skip"), "{out}");
}

#[test]
fn json_output_is_one_object_per_line() {
    let dir = TempDir::new("json");
    dir.file("j.md", "# A\n\n### Skip\n");
    let o = run_in(&dir.0, &["--format", "json", "j.md"], "");
    let out = stdout(&o);
    assert_eq!(out.lines().count(), 1);
    assert!(
        out.starts_with('{') && out.trim_end().ends_with('}'),
        "{out}"
    );
    assert!(out.contains("\"rule\":\"heading-skip\""), "{out}");
}

#[test]
fn stdin_is_checked_and_rules_can_be_listed() {
    let dir = TempDir::new("stdin");
    let o = run_in(&dir.0, &[], "# A\n\n### Skip\n");
    assert!(!o.status.success());
    assert!(
        stdout(&o).starts_with("-:3:1: heading-skip"),
        "{}",
        stdout(&o)
    );

    let o = run_in(&dir.0, &["--list-rules"], "");
    assert!(o.status.success());
    let out = stdout(&o);
    assert!(out.contains("link-file\ton"), "{out}");
    assert!(out.contains("line-length\toff"), "{out}");
}
