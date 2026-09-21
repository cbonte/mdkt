//! End-to-end tests of `mdkt fmt`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdkt"))
        .arg("fmt")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mdkt fmt");
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
        let dir = std::env::temp_dir().join(format!("mdkt-fmt-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
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
fn rewraps_stdin_to_the_given_width() {
    let o = run(&["--width", "20"], "one two three four five six seven\n");
    assert!(o.status.success());
    assert_eq!(stdout(&o), "one two three four\nfive six seven\n");
}

#[test]
fn width_zero_joins_each_paragraph() {
    let o = run(&["--width", "0"], "a\nb\nc\n\nd\ne\n");
    assert_eq!(stdout(&o), "a b c\n\nd e\n");
}

#[test]
fn markup_is_preserved() {
    let md = "See [a link](http://x) and `code  span` and \\* an escape.\n";
    let o = run(&["--width", "20"], md);
    let out = stdout(&o);
    // Link text may wrap, but the destination and the code span may not.
    assert!(out.contains("link](http://x)"), "{out}");
    assert!(out.contains("`code  span`"), "{out}");
    assert!(out.contains("\\*"), "{out}");
    let o = run(&["--width", "0"], md);
    assert_eq!(stdout(&o), md);
}

#[test]
fn bullet_can_be_chosen() {
    let o = run(&["--bullet", "*"], "- one\n- two\n");
    assert_eq!(stdout(&o), "* one\n* two\n");
}

#[test]
fn check_reports_a_file_that_would_change() {
    let dir = TempDir::new("check");
    let path = dir.file("doc.md", "one\ntwo\n");
    let arg = path.to_str().expect("utf-8 path");
    let o = run(&["--check", arg], "");
    assert!(!o.status.success());
    assert_eq!(stdout(&o).trim(), arg);

    let formatted = dir.file("done.md", "one two\n");
    let o = run(&["--check", formatted.to_str().expect("utf-8 path")], "");
    assert!(o.status.success());
    assert!(stdout(&o).is_empty());
}

#[test]
fn write_is_in_place_and_idempotent() {
    let dir = TempDir::new("write");
    let path = dir.file("doc.md", "one\ntwo three\n");
    let arg = path.to_str().expect("utf-8 path");
    assert!(run(&["--write", arg], "").status.success());
    let first = std::fs::read_to_string(&path).expect("read back");
    assert_eq!(first, "one two three\n");
    assert!(run(&["--write", arg], "").status.success());
    assert_eq!(std::fs::read_to_string(&path).expect("read back"), first);
}

#[test]
fn diff_shows_the_changed_lines_only() {
    let dir = TempDir::new("diff");
    let path = dir.file("doc.md", "keep this line\n\none\ntwo\n");
    let o = run(&["--diff", path.to_str().expect("utf-8 path")], "");
    let out = stdout(&o);
    assert!(out.starts_with("--- "), "{out}");
    assert!(out.contains("-one\n-two\n+one two\n"), "{out}");
    assert!(!out.contains("keep this line"), "{out}");
}

#[test]
fn write_from_stdin_is_refused() {
    let o = run(&["--write"], "x\n");
    assert!(!o.status.success());
    let err = String::from_utf8(o.stderr).expect("utf-8 stderr");
    assert!(err.contains("needs a file"), "{err}");
}
