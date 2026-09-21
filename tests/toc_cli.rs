//! End-to-end tests of `mdkt toc`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdkt"))
        .arg("toc")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mdkt toc");
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

const DOC: &str =
    "# Title\n\n<!-- toc -->\n<!-- /toc -->\n\n## One\n\ntext\n\n### Deep\n\n## Two\n";
const TABLE: &str = "- [One](#one)\n  - [Deep](#deep)\n- [Two](#two)\n";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("mdkt-toc-{}-{name}", std::process::id()));
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
fn prints_the_table_alone_without_markers() {
    let o = run(&[], "# Title\n\n## One\n\n## Two\n");
    assert!(o.status.success());
    assert_eq!(stdout(&o), "- [One](#one)\n- [Two](#two)\n");
}

#[test]
fn fills_the_marker_block_on_stdout() {
    let o = run(&[], DOC);
    assert!(o.status.success());
    let expected = DOC.replace("<!-- toc -->\n", &format!("<!-- toc -->\n\n{TABLE}\n"));
    assert_eq!(stdout(&o), expected);
}

#[test]
fn write_updates_in_place_and_is_idempotent() {
    let dir = TempDir::new("write");
    let path = dir.file("doc.md", DOC);
    let arg = path.to_str().expect("utf-8 path");
    assert!(run(&["--write", arg], "").status.success());
    let first = std::fs::read_to_string(&path).expect("read back");
    assert!(first.contains(TABLE));
    assert!(run(&["--write", arg], "").status.success());
    assert_eq!(std::fs::read_to_string(&path).expect("read back"), first);
}

#[test]
fn check_reports_a_stale_table() {
    let dir = TempDir::new("check");
    let stale = dir.file("stale.md", DOC);
    let arg = stale.to_str().expect("utf-8 path");
    let o = run(&["--check", arg], "");
    assert!(!o.status.success());
    assert_eq!(stdout(&o).trim(), arg);

    assert!(run(&["--write", arg], "").status.success());
    let o = run(&["--check", arg], "");
    assert!(o.status.success());
    assert!(stdout(&o).is_empty());
}

#[test]
fn depth_and_bullet_options() {
    let o = run(&["--max-depth", "2", "--bullet", "*"], DOC);
    let out = stdout(&o);
    assert!(out.contains("* [One](#one)\n* [Two](#two)\n"), "{out}");
    assert!(!out.contains("(#deep)"), "{out}");
}

#[test]
fn write_without_markers_fails() {
    let dir = TempDir::new("nomarker");
    let path = dir.file("plain.md", "# Title\n\n## One\n");
    let o = run(&["--write", path.to_str().expect("utf-8 path")], "");
    assert!(!o.status.success());
    let err = String::from_utf8(o.stderr).expect("utf-8 stderr");
    assert!(err.contains("markers"), "{err}");
}
