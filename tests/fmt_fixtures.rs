//! Golden tests: every `tests/fixtures/fmt/*.md` formatted at a fixed width.
//! Run with `UPDATE_FIXTURES=1` to rewrite the expected files.

use std::fs;
use std::path::{Path, PathBuf};

use mdkt::cmd::fmt::{FmtOptions, format};

const WIDTH: usize = 60;

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fmt");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("fixtures dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .filter(|p| !p.to_string_lossy().ends_with(".out.md"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
}

#[test]
fn golden_outputs() {
    let updating = std::env::var_os("UPDATE_FIXTURES").is_some();
    for md in fixtures() {
        let src = fs::read_to_string(&md).expect("read fixture");
        let out = format(&src, &FmtOptions::with_width(WIDTH));
        let expected_path = md.with_extension("out.md");
        if updating {
            fs::write(&expected_path, &out).expect("write expectation");
            continue;
        }
        let expected = fs::read_to_string(&expected_path).unwrap_or_else(|_| {
            panic!(
                "missing {}, run with UPDATE_FIXTURES=1",
                expected_path.display()
            )
        });
        assert_eq!(out, expected, "{}", md.display());
    }
}
