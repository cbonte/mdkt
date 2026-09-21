//! Findings and the rules that produce them.

use std::fmt;

/// A rule, named as it appears on the command line and in the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Rule {
    LinkFile,
    LinkAnchor,
    LinkRef,
    HeadingSkip,
    HeadingDup,
    HeadingSingleH1,
    ImageAlt,
    FenceLang,
    TrailingSpace,
    LineLength,
}

/// Rules that run unless disabled. The others need `--enable`.
pub const DEFAULT_RULES: [Rule; 6] = [
    Rule::LinkFile,
    Rule::LinkAnchor,
    Rule::LinkRef,
    Rule::HeadingSkip,
    Rule::HeadingDup,
    Rule::ImageAlt,
];

impl Rule {
    pub fn name(self) -> &'static str {
        match self {
            Rule::LinkFile => "link-file",
            Rule::LinkAnchor => "link-anchor",
            Rule::LinkRef => "link-ref",
            Rule::HeadingSkip => "heading-skip",
            Rule::HeadingDup => "heading-dup",
            Rule::HeadingSingleH1 => "heading-single-h1",
            Rule::ImageAlt => "image-alt",
            Rule::FenceLang => "fence-lang",
            Rule::TrailingSpace => "trailing-space",
            Rule::LineLength => "line-length",
        }
    }

    pub fn on_by_default(self) -> bool {
        DEFAULT_RULES.contains(&self)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub column: usize,
    pub rule: Rule,
    pub message: String,
}

impl Finding {
    /// Formats the finding as `path:line:column: rule: message`.
    pub fn render(&self, path: &str) -> String {
        format!(
            "{path}:{}:{}: {}: {}",
            self.line, self.column, self.rule, self.message
        )
    }

    /// Formats the finding as one JSON object on a single line.
    pub fn render_json(&self, path: &str) -> String {
        format!(
            "{{\"file\":{},\"line\":{},\"column\":{},\"rule\":{},\"message\":{}}}",
            json_string(path),
            self.line,
            self.column,
            json_string(self.rule.name()),
            json_string(&self.message)
        )
    }
}

/// Quotes a string as a JSON scalar, escaping what the format requires.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering() {
        let f = Finding {
            line: 3,
            column: 7,
            rule: Rule::LinkFile,
            message: "missing.md does not exist".into(),
        };
        assert_eq!(
            f.render("a.md"),
            "a.md:3:7: link-file: missing.md does not exist"
        );
        assert_eq!(
            f.render_json("a.md"),
            "{\"file\":\"a.md\",\"line\":3,\"column\":7,\"rule\":\"link-file\",\"message\":\"missing.md does not exist\"}"
        );
    }

    #[test]
    fn json_escapes_quotes_and_controls() {
        assert_eq!(json_string("a\"b\\c\nd\te"), "\"a\\\"b\\\\c\\nd\\te\"");
        assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
    }

    #[test]
    fn opt_in_rules_are_not_default() {
        assert!(Rule::LinkFile.on_by_default());
        assert!(!Rule::LineLength.on_by_default());
        assert!(!Rule::FenceLang.on_by_default());
    }
}
