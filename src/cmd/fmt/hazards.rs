//! Keeping a rewrapped line from starting a new block.
//!
//! A line break inserted by the formatter must not turn prose into markup. A word that
//! could open a block at the start of a line is merged with the word before it, so no
//! break can land in front of it. Merging never changes the text, unlike escaping.

use crate::cmd::fmt::inline::Atom;

/// True when `word` at the start of a line would open a block or a table row.
pub fn starts_a_block(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    // A run of thematic break, setext underline or table delimiter characters.
    if word
        .chars()
        .all(|c| matches!(c, '-' | '*' | '_' | '=' | ':' | '|'))
    {
        return true;
    }
    if word == "+" || word.starts_with('>') || word.starts_with('|') {
        return true;
    }
    if word.len() <= 6 && word.chars().all(|c| c == '#') {
        return true;
    }
    if word.starts_with("```") || word.starts_with("~~~") {
        return true;
    }
    // An HTML block opener: '<' then a name, a closing slash, or a declaration.
    if let Some(rest) = word.strip_prefix('<')
        && rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/' || c == '!' || c == '?')
    {
        return true;
    }
    if word.starts_with("[^") && word.contains("]:") {
        return true;
    }
    is_list_marker(word)
}

/// True for an ordered list marker such as `1.` or `12)`.
fn is_list_marker(word: &str) -> bool {
    let digits: String = word.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 9 {
        return false;
    }
    matches!(&word[digits.len()..], "." | ")")
}

/// True when a line ending in `word` would become a hard break.
/// An odd number of trailing backslashes leaves one unescaped.
pub fn ends_a_line_break(word: &str) -> bool {
    word.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

/// Merges the words that must not be separated by a line break.
pub fn merge(atoms: Vec<Atom>) -> Vec<Atom> {
    let mut out: Vec<Atom> = Vec::with_capacity(atoms.len());
    for atom in atoms {
        // A break between the two words would either open a block or leave a trailing
        // backslash, so the pair has to travel together.
        let merge_back = match (&atom, out.last(), out.iter().rev().nth(1)) {
            (Atom::Word(word), Some(Atom::Space), Some(Atom::Word(prev))) => {
                starts_a_block(word) || ends_a_line_break(prev)
            }
            _ => false,
        };
        if merge_back {
            out.pop();
            let Some(Atom::Word(prev)) = out.pop() else {
                unreachable!("checked above")
            };
            let Atom::Word(word) = atom else {
                unreachable!("checked above")
            };
            out.push(Atom::Word(format!("{prev} {word}")));
            continue;
        }
        out.push(atom);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<Atom> {
        let mut out = Vec::new();
        for (i, w) in text.split(' ').enumerate() {
            if i > 0 {
                out.push(Atom::Space);
            }
            out.push(Atom::Word(w.to_string()));
        }
        out
    }

    fn merged(text: &str) -> Vec<String> {
        merge(words(text))
            .into_iter()
            .filter_map(|a| match a {
                Atom::Word(w) => Some(w),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn block_openers() {
        for word in [
            "-", "*", "---", "***", "===", ":--", "|", "+", "#", "######", "> x", "1.", "9)",
            "```rust", "~~~", "<div", "</p>", "<!--", "[^a]: x",
        ] {
            assert!(starts_a_block(word), "{word}");
        }
        for word in [
            "a",
            "-a",
            "1.5",
            "1234567890.",
            "#######",
            "<3",
            "x-",
            "**bold**",
        ] {
            assert!(!starts_a_block(word), "{word}");
        }
    }

    #[test]
    fn trailing_backslashes() {
        assert!(ends_a_line_break("a\\"));
        assert!(!ends_a_line_break("a\\\\"));
        assert!(ends_a_line_break("a\\\\\\"));
        assert!(!ends_a_line_break("a"));
    }

    #[test]
    fn hazards_join_the_previous_word() {
        assert_eq!(merged("see 1. above"), ["see 1.", "above"]);
        assert_eq!(merged("a - b"), ["a -", "b"]);
        assert_eq!(merged("C # is"), ["C #", "is"]);
        assert_eq!(merged("a > b"), ["a >", "b"]);
        assert_eq!(merged("plain words here"), ["plain", "words", "here"]);
    }

    #[test]
    fn a_leading_hazard_is_left_alone() {
        assert_eq!(merged("- already a list"), ["-", "already", "a", "list"]);
    }

    #[test]
    fn a_trailing_backslash_keeps_the_next_word() {
        assert_eq!(merged("a\\ b c"), ["a\\ b", "c"]);
    }
}
