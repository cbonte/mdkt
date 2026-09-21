//! The checks themselves. Each one walks the parsed document and reports findings.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::slug::{self, github_anchor};
use crate::source::LineIndex;
use crate::tree::parser_options;
use pulldown_cmark::{BrokenLink, CodeBlockKind, Event, LinkType, Parser, Tag, TagEnd};
use std::cell::RefCell;

use crate::cmd::lint::finding::{Finding, Rule};

/// Everything a check needs about one file.
pub struct Source<'a> {
    pub path: &'a Path,
    pub text: &'a str,
    pub index: LineIndex,
}

impl<'a> Source<'a> {
    pub fn new(path: &'a Path, text: &'a str) -> Source<'a> {
        Source {
            path,
            text,
            index: LineIndex::new(text),
        }
    }

    fn at(&self, offset: usize) -> (usize, usize) {
        self.index.line_col(self.text, offset)
    }

    fn finding(&self, offset: usize, rule: Rule, message: String) -> Finding {
        let (line, column) = self.at(offset);
        Finding {
            line,
            column,
            rule,
            message,
        }
    }
}

/// Anchors a Markdown file offers: its heading anchors plus explicit HTML names.
pub fn anchors(text: &str) -> HashSet<String> {
    let doc = crate::document(text, &crate::RenderOptions::default());
    let mut set: HashSet<String> = slug::outline(&doc).into_iter().map(|e| e.anchor).collect();
    for tag in crate::html::tokenize(text) {
        if let crate::html::Token::Tag(tag) = tag {
            for name in ["id", "name"] {
                if let Some(value) = tag.attr(name) {
                    set.insert(value);
                }
            }
        }
    }
    set
}

/// Splits a link target into its path part and its anchor.
fn split_target(dest: &str) -> (&str, Option<&str>) {
    match dest.split_once('#') {
        Some((path, anchor)) => (path, Some(anchor)),
        None => (dest, None),
    }
}

/// True for a target that points outside the file tree.
fn is_external(dest: &str) -> bool {
    dest.contains("://") || dest.starts_with("mailto:") || dest.starts_with("tel:")
}

/// Percent-decodes the few sequences a local path can carry.
fn decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Checks every link and image target that stays inside the file tree.
fn check_targets(src: &Source<'_>, enabled: &HashSet<Rule>, out: &mut Vec<Finding>) {
    let dir = src.path.parent().unwrap_or(Path::new("."));
    let mut cache: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let own_anchors = anchors(src.text);
    for (event, range) in Parser::new_ext(src.text, parser_options()).into_offset_iter() {
        let (dest, link_type, is_image) = match &event {
            Event::Start(Tag::Link {
                dest_url,
                link_type,
                ..
            }) => (dest_url.to_string(), *link_type, false),
            Event::Start(Tag::Image {
                dest_url,
                link_type,
                ..
            }) => (dest_url.to_string(), *link_type, true),
            _ => continue,
        };
        // An email autolink has no scheme in its target, so it would look like a path.
        if matches!(link_type, LinkType::Email) || is_external(&dest) || dest.is_empty() {
            continue;
        }
        let (path_part, anchor) = split_target(&dest);
        let path_part = path_part.split('?').next().unwrap_or(path_part);
        let target = if path_part.is_empty() {
            src.path.to_path_buf()
        } else {
            dir.join(decode_path(path_part))
        };
        if !path_part.is_empty() && enabled.contains(&Rule::LinkFile) && !target.exists() {
            out.push(src.finding(
                range.start,
                Rule::LinkFile,
                format!("{path_part} does not exist"),
            ));
            continue;
        }
        let Some(anchor) = anchor.filter(|a| !a.is_empty()) else {
            continue;
        };
        if is_image || !enabled.contains(&Rule::LinkAnchor) {
            continue;
        }
        let known = if path_part.is_empty() {
            Some(&own_anchors)
        } else if target.extension().is_some_and(|e| e == "md") {
            let entry = cache.entry(target.clone()).or_insert_with(|| {
                std::fs::read_to_string(&target)
                    .map(|t| anchors(&t))
                    .unwrap_or_default()
            });
            Some(&*entry)
        } else {
            None
        };
        if let Some(known) = known
            && !known.contains(anchor)
        {
            let where_ = if path_part.is_empty() {
                "this file".to_string()
            } else {
                path_part.to_string()
            };
            out.push(src.finding(
                range.start,
                Rule::LinkAnchor,
                format!("no heading '{anchor}' in {where_}"),
            ));
        }
    }
}

/// Checks reference links without a definition, and definitions nobody uses.
///
/// An undefined reference produces no link event, so it is collected through the broken
/// link callback. Returning `None` there keeps the text rendered as written.
fn check_references(src: &Source<'_>, out: &mut Vec<Finding>) {
    let defined: HashMap<String, usize> = Parser::new_ext(src.text, parser_options())
        .reference_definitions()
        .iter()
        .map(|(label, def)| (label.to_lowercase(), def.span.start))
        .collect();
    let broken: RefCell<Vec<(usize, String)>> = RefCell::new(Vec::new());
    let callback =
        |link: BrokenLink<'_>| -> Option<(pulldown_cmark::CowStr<'_>, pulldown_cmark::CowStr<'_>)> {
            // The callback reports the form that was attempted. A shortcut is often just
            // bracketed prose, so only the explicit forms count as a broken reference.
            if matches!(link.link_type, LinkType::Reference | LinkType::Collapsed) {
                broken
                    .borrow_mut()
                    .push((link.span.start, link.reference.to_string()));
            }
            None
        };
    let parser = Parser::new_with_broken_link_callback(src.text, parser_options(), Some(callback));
    let mut used: HashSet<String> = HashSet::new();
    for (event, _) in parser.into_offset_iter() {
        let (link_type, id) = match &event {
            Event::Start(Tag::Link { link_type, id, .. })
            | Event::Start(Tag::Image { link_type, id, .. }) => (*link_type, id.to_string()),
            _ => continue,
        };
        if matches!(
            link_type,
            LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
        ) {
            used.insert(id.to_lowercase());
        }
    }
    for (offset, label) in broken.into_inner() {
        out.push(src.finding(
            offset,
            Rule::LinkRef,
            format!("reference '{label}' has no definition"),
        ));
    }
    let mut unused: Vec<(&String, &usize)> = defined
        .iter()
        .filter(|(label, _)| !used.contains(*label))
        .collect();
    unused.sort_by_key(|(_, offset)| **offset);
    for (label, offset) in unused {
        out.push(src.finding(
            *offset,
            Rule::LinkRef,
            format!("definition '{label}' is never used"),
        ));
    }
}

/// Checks heading levels, duplicate anchors and the number of level one headings.
fn check_headings(src: &Source<'_>, enabled: &HashSet<Rule>, out: &mut Vec<Finding>) {
    let mut previous: Option<u8> = None;
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut anchor_count: HashMap<String, usize> = HashMap::new();
    let mut h1 = 0;
    let mut current: Option<(u8, usize, String)> = None;
    for (event, range) in Parser::new_ext(src.text, parser_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some((level as u8, range.start, String::new()));
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, _, text)) = &mut current {
                    text.push_str(&t);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                let Some((level, offset, text)) = current.take() else {
                    continue;
                };
                if level == 1 {
                    h1 += 1;
                    if h1 > 1 && enabled.contains(&Rule::HeadingSingleH1) {
                        out.push(src.finding(
                            offset,
                            Rule::HeadingSingleH1,
                            format!("a second level one heading: '{text}'"),
                        ));
                    }
                }
                if let Some(prev) = previous
                    && level > prev + 1
                    && enabled.contains(&Rule::HeadingSkip)
                {
                    out.push(src.finding(
                        offset,
                        Rule::HeadingSkip,
                        format!("level {level} follows level {prev}"),
                    ));
                }
                previous = Some(level);
                let anchor = github_anchor(&text, &mut seen);
                let base = anchor.rsplit_once('-').map_or(anchor.clone(), |(b, n)| {
                    if n.chars().all(|c| c.is_ascii_digit()) {
                        b.to_string()
                    } else {
                        anchor.clone()
                    }
                });
                let count = anchor_count.entry(base.clone()).or_insert(0);
                *count += 1;
                if *count > 1 && enabled.contains(&Rule::HeadingDup) {
                    out.push(src.finding(
                        offset,
                        Rule::HeadingDup,
                        format!("'{text}' repeats the anchor '{base}'"),
                    ));
                }
            }
            _ => {}
        }
    }
}

/// Checks images without alt text and fences without a language.
fn check_blocks(src: &Source<'_>, enabled: &HashSet<Rule>, out: &mut Vec<Finding>) {
    let mut image: Option<(usize, String)> = None;
    for (event, range) in Parser::new_ext(src.text, parser_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { .. }) => image = Some((range.start, String::new())),
            Event::Text(t) | Event::Code(t) => {
                if let Some((_, alt)) = &mut image {
                    alt.push_str(&t);
                }
            }
            Event::End(TagEnd::Image) => {
                if let Some((offset, alt)) = image.take()
                    && alt.trim().is_empty()
                    && enabled.contains(&Rule::ImageAlt)
                {
                    out.push(src.finding(
                        offset,
                        Rule::ImageAlt,
                        "image without alt text".to_string(),
                    ));
                }
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if enabled.contains(&Rule::FenceLang) && info.trim().is_empty() =>
            {
                out.push(src.finding(
                    range.start,
                    Rule::FenceLang,
                    "fenced code block without a language".to_string(),
                ));
            }
            _ => {}
        }
    }
}

/// Lines that hold verbatim content, where whitespace and length rules do not apply.
fn verbatim_lines(src: &Source<'_>) -> HashSet<usize> {
    let mut lines = HashSet::new();
    for (event, range) in Parser::new_ext(src.text, parser_options()).into_offset_iter() {
        let covered = matches!(
            event,
            Event::Start(
                Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::MetadataBlock(_) | Tag::Table(_)
            )
        );
        if covered {
            let first = src.index.line_of(range.start);
            let last = src.index.line_of(range.end.saturating_sub(1));
            lines.extend(first..=last);
        }
    }
    lines
}

/// Checks trailing whitespace and line length.
fn check_lines(
    src: &Source<'_>,
    enabled: &HashSet<Rule>,
    line_length: usize,
    out: &mut Vec<Finding>,
) {
    if !enabled.contains(&Rule::TrailingSpace) && !enabled.contains(&Rule::LineLength) {
        return;
    }
    let verbatim = verbatim_lines(src);
    for line_no in 1..=src.index.line_count() {
        let range = src.index.line_range(src.text, line_no);
        let line = &src.text[range.clone()];
        if verbatim.contains(&line_no) {
            continue;
        }
        if enabled.contains(&Rule::TrailingSpace) {
            let trimmed = line.trim_end_matches([' ', '\t']);
            let trailing = &line[trimmed.len()..];
            // Exactly two spaces are a hard break, and a blank line has nothing to trim.
            let is_break = trailing == "  " && !trimmed.is_empty();
            if !trailing.is_empty() && !is_break {
                out.push(Finding {
                    line: line_no,
                    column: trimmed.chars().count() + 1,
                    rule: Rule::TrailingSpace,
                    message: format!("{} trailing whitespace characters", trailing.len()),
                });
            }
        }
        if enabled.contains(&Rule::LineLength) {
            let width = line.chars().count();
            if width > line_length && !line.contains("://") {
                out.push(Finding {
                    line: line_no,
                    column: line_length + 1,
                    rule: Rule::LineLength,
                    message: format!("line is {width} characters, over {line_length}"),
                });
            }
        }
    }
}

/// Runs the enabled rules over one file and returns the findings in source order.
pub fn check(path: &Path, text: &str, enabled: &HashSet<Rule>, line_length: usize) -> Vec<Finding> {
    let src = Source::new(path, text);
    let mut out = Vec::new();
    check_targets(&src, enabled, &mut out);
    if enabled.contains(&Rule::LinkRef) {
        check_references(&src, &mut out);
    }
    check_headings(&src, enabled, &mut out);
    check_blocks(&src, enabled, &mut out);
    check_lines(&src, enabled, line_length, &mut out);
    out.sort_by_key(|f| (f.line, f.column, f.rule));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(text: &str, rules: &[Rule]) -> Vec<String> {
        let enabled: HashSet<Rule> = rules.iter().copied().collect();
        check(Path::new("doc.md"), text, &enabled, 80)
            .into_iter()
            .map(|f| format!("{}:{}: {}: {}", f.line, f.column, f.rule, f.message))
            .collect()
    }

    #[test]
    fn heading_levels_and_duplicates() {
        let out = findings(
            "# A\n\n### C\n\n## B\n\n## B\n",
            &[Rule::HeadingSkip, Rule::HeadingDup],
        );
        assert_eq!(
            out,
            [
                "3:1: heading-skip: level 3 follows level 1",
                "7:1: heading-dup: 'B' repeats the anchor 'b'"
            ]
        );
    }

    #[test]
    fn missing_and_unused_references() {
        let out = findings(
            "see [a][known] and [b][gone]\n\n[known]: http://x\n[spare]: http://y\n",
            &[Rule::LinkRef],
        );
        assert_eq!(
            out,
            [
                "1:20: link-ref: reference 'gone' has no definition",
                "4:1: link-ref: definition 'spare' is never used"
            ]
        );
    }

    #[test]
    fn anchors_within_the_same_file() {
        let out = findings(
            "# Title\n\n[ok](#title) [bad](#nope)\n",
            &[Rule::LinkAnchor],
        );
        assert_eq!(out, ["3:14: link-anchor: no heading 'nope' in this file"]);
    }

    #[test]
    fn explicit_html_names_count_as_anchors() {
        let out = findings(
            "<a name=\"here\"></a>\n\n[go](#here)\n",
            &[Rule::LinkAnchor],
        );
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn email_autolinks_are_not_paths() {
        let out = findings("write to <someone@example.com> now\n", &[Rule::LinkFile]);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn images_need_alt_text() {
        let out = findings("![](a.png) ![ok](b.png)\n", &[Rule::ImageAlt]);
        assert_eq!(out, ["1:1: image-alt: image without alt text"]);
    }

    #[test]
    fn hard_breaks_are_not_trailing_whitespace() {
        let out = findings("a  \nb \nc\t\n\nd\n", &[Rule::TrailingSpace]);
        assert_eq!(
            out,
            [
                "2:2: trailing-space: 1 trailing whitespace characters",
                "3:2: trailing-space: 1 trailing whitespace characters"
            ]
        );
    }

    #[test]
    fn verbatim_blocks_escape_the_line_rules() {
        let text = "```\ncode   \n```\n\ntext   \n";
        let out = findings(text, &[Rule::TrailingSpace]);
        assert_eq!(
            out,
            ["5:5: trailing-space: 3 trailing whitespace characters"]
        );
    }

    #[test]
    fn long_lines_holding_a_url_are_left_alone() {
        let long = "x".repeat(90);
        let url = format!("see https://example.com/{}", "y".repeat(90));
        let text = format!("{long}\n\n{url}\n");
        let out = findings(&text, &[Rule::LineLength]);
        assert_eq!(out.len(), 1);
        assert!(out[0].starts_with("1:81: line-length:"), "{out:?}");
    }

    #[test]
    fn fences_without_a_language() {
        let out = findings("```\nx\n```\n\n```sh\ny\n```\n", &[Rule::FenceLang]);
        assert_eq!(
            out,
            ["1:1: fence-lang: fenced code block without a language"]
        );
    }
}
