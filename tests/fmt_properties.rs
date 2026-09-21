//! Properties every formatting must hold, checked over the fixture corpus.

use std::fs;
use std::path::PathBuf;

use mdkt::cmd::fmt::{FmtOptions, format};
use mdkt::tree::parser_options;
use mdkt::wrap::display_width;
use pulldown_cmark::{Event, Parser};

const WIDTH: usize = 60;

fn inputs() -> Vec<(String, String)> {
    let mut dirs = vec![
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fmt"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../mdkt/tests/fixtures"),
    ];
    dirs.retain(|d| d.is_dir());
    let mut out = Vec::new();
    for dir in dirs {
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .expect("fixture directory")
            .map(|e| e.expect("entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .collect();
        files.sort();
        for file in files {
            let name = file
                .strip_prefix(env!("CARGO_MANIFEST_DIR"))
                .unwrap_or(&file)
                .display()
                .to_string();
            out.push((name, fs::read_to_string(&file).expect("read fixture")));
        }
    }
    assert!(!out.is_empty(), "no fixtures found");
    out
}

/// Event stream with the differences a rewrap is allowed to make removed: a soft break
/// becomes a space, adjacent texts merge and whitespace runs collapse.
fn normalize(md: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut Vec<String>| {
        let joined: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !joined.is_empty() {
            out.push(format!("Text({joined})"));
        }
        text.clear();
    };
    for event in Parser::new_ext(md, parser_options()) {
        match event {
            Event::Text(t) => text.push_str(&t),
            Event::SoftBreak => text.push(' '),
            other => {
                flush(&mut text, &mut out);
                out.push(format!("{other:?}"));
            }
        }
    }
    flush(&mut text, &mut out);
    out
}

/// One-based numbers of the lines holding verbatim content, which is never wrapped.
fn verbatim_lines(md: &str) -> std::collections::HashSet<usize> {
    use mdkt::source::LineIndex;
    use pulldown_cmark::Tag;
    let index = LineIndex::new(md);
    let mut lines = std::collections::HashSet::new();
    for (event, range) in Parser::new_ext(md, parser_options()).into_offset_iter() {
        if matches!(
            event,
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_))
        ) {
            let first = index.line_of(range.start);
            let last = index.line_of(range.end.saturating_sub(1));
            lines.extend(first..=last);
        }
    }
    lines
}

fn opts() -> FmtOptions {
    FmtOptions::with_width(WIDTH)
}

#[test]
fn formatting_is_idempotent() {
    for (name, src) in inputs() {
        let once = format(&src, &opts());
        let twice = format(&once, &opts());
        assert_eq!(once, twice, "{name} changes on a second pass");
    }
}

#[test]
fn the_event_stream_survives() {
    for (name, src) in inputs() {
        let out = format(&src, &opts());
        let before = normalize(&src);
        let after = normalize(&out);
        if before != after {
            let first = before
                .iter()
                .zip(&after)
                .position(|(a, b)| a != b)
                .unwrap_or(before.len().min(after.len()));
            panic!(
                "{name}: event {first} differs\n  before: {:?}\n  after:  {:?}\n--- output ---\n{out}",
                before.get(first),
                after.get(first)
            );
        }
    }
}

#[test]
fn lines_fit_the_width_unless_one_run_cannot() {
    for (name, src) in inputs() {
        let out = format(&src, &opts());
        let verbatim = verbatim_lines(&out);
        for (i, line) in out.lines().enumerate() {
            if display_width(line) <= WIDTH {
                continue;
            }
            // A table and verbatim content are never wrapped.
            if line.trim_start().starts_with('|') || verbatim.contains(&(i + 1)) {
                continue;
            }
            // Otherwise the overflow must come from one run that cannot be broken:
            // dropping the last run has to bring the line back within the width.
            let without_last = line
                .rsplit_once(' ')
                .map(|(head, _)| head)
                .unwrap_or_default();
            assert!(
                display_width(without_last) <= WIDTH,
                "{name} line {}: {line:?}",
                i + 1
            );
        }
    }
}

#[test]
fn no_trailing_whitespace_except_hard_breaks() {
    for (name, src) in inputs() {
        let out = format(&src, &opts());
        for (i, line) in out.lines().enumerate() {
            let trimmed = line.trim_end_matches(' ');
            let trailing = line.len() - trimmed.len();
            assert!(
                trailing == 0 || (trailing == 2 && !trimmed.is_empty()),
                "{name} line {}: {line:?}",
                i + 1
            );
        }
    }
}

#[test]
fn an_empty_document_stays_empty() {
    assert_eq!(format("", &opts()), "");
    assert_eq!(format("\n\n\n", &opts()), "");
}

#[test]
fn width_zero_puts_a_paragraph_on_one_line() {
    let opts = FmtOptions::with_width(0);
    let out = format("one\ntwo\nthree\n", &opts);
    assert_eq!(out, "one two three\n");
}
