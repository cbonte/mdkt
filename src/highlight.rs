//! Syntax highlighting of code blocks, producing styled lines.

use anyhow::{Result, anyhow};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::model::{Line, Segment};
use crate::style::{Color, Style};

pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: syntect::highlighting::Theme,
    plain: Style,
}

/// Names of the bundled colour themes, sorted.
pub fn theme_names() -> Vec<String> {
    let mut names: Vec<String> = ThemeSet::load_defaults().themes.into_keys().collect();
    names.sort();
    names
}

fn convert(s: syntect::highlighting::Style, base: Style) -> Style {
    let mut out = base;
    if s.foreground.a != 0 {
        out.fg = Some(Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b));
    }
    out.bold |= s.font_style.contains(FontStyle::BOLD);
    out.italic |= s.font_style.contains(FontStyle::ITALIC);
    out.underline |= s.font_style.contains(FontStyle::UNDERLINE);
    out
}

/// Splits `code` into lines of one segment each, in `style`.
pub fn plain_lines(code: &str, style: Style) -> Vec<Line> {
    code.lines().map(|l| vec![Segment::new(l, style)]).collect()
}

impl Highlighter {
    pub fn new(theme_name: &str, plain: Style) -> Result<Highlighter> {
        let mut themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .remove(theme_name)
            .ok_or_else(|| anyhow!("unknown code theme '{theme_name}', see --list-themes"))?;
        Ok(Highlighter {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            theme,
            plain,
        })
    }

    pub fn highlight(&self, lang: Option<&str>, code: &str) -> Vec<Line> {
        let syntax = lang
            .and_then(|l| self.syntaxes.find_syntax_by_token(l))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let mut hl = HighlightLines::new(syntax, &self.theme);
        LinesWithEndings::from(code)
            .map(|line| {
                let Ok(ranges) = hl.highlight_line(line, &self.syntaxes) else {
                    return vec![Segment::new(line.trim_end_matches('\n'), self.plain)];
                };
                let mut out: Line = Vec::new();
                for (style, text) in ranges {
                    let text = text.trim_end_matches(['\n', '\r']);
                    if text.is_empty() {
                        continue;
                    }
                    let style = convert(style, self.plain);
                    match out.last_mut() {
                        Some(last) if last.style == style => last.text.push_str(text),
                        _ => out.push(Segment::new(text, style)),
                    }
                }
                out
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_with_colours() {
        let hl = Highlighter::new(crate::options::defaults::CODE_THEME, Style::PLAIN).unwrap();
        let lines = hl.highlight(Some("rust"), "fn main() {}\nlet x = 1;\n");
        assert_eq!(lines.len(), 2);
        let text: String = lines[0].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "fn main() {}");
        assert!(lines[0].iter().any(|s| s.style.fg.is_some()));
    }

    #[test]
    fn unknown_language_falls_back_to_plain() {
        let hl = Highlighter::new(crate::options::defaults::CODE_THEME, Style::PLAIN).unwrap();
        let lines = hl.highlight(Some("no-such-lang"), "x\n");
        assert_eq!(lines[0][0].text, "x");
    }

    #[test]
    fn unknown_theme_is_an_error() {
        assert!(Highlighter::new("no-such-theme", Style::PLAIN).is_err());
    }
}
