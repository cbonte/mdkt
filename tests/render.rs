//! Fixture tests: every `tests/fixtures/render/*.md` is rendered at a fixed width and compared with
//! `*.join.txt` and, when soft breaks change the output, `*.preserve.txt`.
//! Run with `UPDATE_FIXTURES=1` to rewrite the expected files.

use std::fs;
use std::path::{Path, PathBuf};

use mdkt::options::LinkMode;
use mdkt::style::ColorLevel;
use mdkt::wrap::{SoftBreaks, display_width};
use mdkt::{RenderOptions, render_to_string};

const WIDTH: usize = 40;

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("fixtures dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
}

fn updating() -> bool {
    std::env::var_os("UPDATE_FIXTURES").is_some()
}

fn expected_path(md: &Path, kind: &str) -> PathBuf {
    md.with_extension(format!("{kind}.txt"))
}

fn check(md: &Path, kind: &str, actual: &str) {
    let path = expected_path(md, kind);
    if updating() {
        fs::write(&path, actual).expect("write fixture");
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing {}, run with UPDATE_FIXTURES=1", path.display()));
    if expected != actual {
        let mut report = format!("mismatch for {}\n", path.display());
        for (i, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
            if e != a {
                report.push_str(&format!(
                    "line {}:\n  expected: {e:?}\n  actual:   {a:?}\n",
                    i + 1
                ));
            }
        }
        report.push_str(&format!(
            "expected {} lines, actual {} lines",
            expected.lines().count(),
            actual.lines().count()
        ));
        panic!("{report}");
    }
}

fn opts(soft_breaks: SoftBreaks) -> RenderOptions {
    RenderOptions {
        soft_breaks,
        ..RenderOptions::plain(WIDTH)
    }
}

#[test]
fn fixtures_join() {
    for md in fixtures() {
        let src = fs::read_to_string(&md).unwrap();
        let out = render_to_string(&src, &opts(SoftBreaks::Join)).unwrap();
        check(&md, "join", &out);
    }
}

#[test]
fn fixtures_preserve() {
    for md in fixtures() {
        let src = fs::read_to_string(&md).unwrap();
        let join = render_to_string(&src, &opts(SoftBreaks::Join)).unwrap();
        let preserve = render_to_string(&src, &opts(SoftBreaks::Preserve)).unwrap();
        let path = expected_path(&md, "preserve");
        if updating() {
            if preserve == join {
                let _ = fs::remove_file(&path);
            } else {
                fs::write(&path, &preserve).unwrap();
            }
        } else if path.exists() {
            check(&md, "preserve", &preserve);
        } else {
            assert_eq!(
                preserve,
                join,
                "{}: preserve output differs from join",
                md.display()
            );
        }
    }
}

/// Removes SGR and OSC 8 sequences.
fn strip_escapes(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            }
            Some(']') => {
                let mut prev = '\0';
                for c in chars.by_ref() {
                    if prev == '\x1b' && c == '\\' {
                        break;
                    }
                    prev = c;
                }
            }
            _ => panic!("unexpected escape in {s:?}"),
        }
    }
    out
}

#[test]
fn coloured_output_matches_plain_and_fits_width() {
    for md in fixtures() {
        let src = fs::read_to_string(&md).unwrap();
        let plain = render_to_string(&src, &opts(SoftBreaks::Join)).unwrap();
        let coloured = RenderOptions {
            color: ColorLevel::TrueColor,
            ..opts(SoftBreaks::Join)
        };
        let out = render_to_string(&src, &coloured).unwrap();
        let stripped = strip_escapes(&out);
        assert_eq!(stripped, plain, "{}", md.display());
        for line in stripped.lines() {
            assert!(
                display_width(line) <= WIDTH,
                "{}: line wider than {WIDTH}: {line:?}",
                md.display()
            );
        }
    }
}

#[test]
fn osc8_links_wrap_the_text_only() {
    let o = RenderOptions {
        color: ColorLevel::TrueColor,
        links: LinkMode::Osc8,
        ..opts(SoftBreaks::Join)
    };
    let out = render_to_string("See [the docs](https://example.com/docs) now.\n", &o).unwrap();
    assert!(
        out.contains("\x1b]8;;https://example.com/docs\x1b\\the docs"),
        "{out:?}"
    );
    assert!(out.contains("\x1b]8;;\x1b\\ now."), "{out:?}");
    assert_eq!(strip_escapes(&out), "See the docs now.\n");
}
