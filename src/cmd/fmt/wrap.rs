//! Greedy wrapping of atoms into lines.

use crate::wrap::display_width;

use crate::cmd::fmt::inline::Atom;

/// Width meaning "never wrap", used by `--width 0`.
pub const UNLIMITED: usize = usize::MAX;

/// Wraps `atoms` into lines of at most `width` columns. A word wider than the line is
/// left whole: breaking it would change the markup.
pub fn wrap(atoms: &[Atom], width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut pending_space = false;
    for atom in atoms {
        match atom {
            Atom::Space => pending_space = !cur.is_empty(),
            Atom::HardBreak(kind) => {
                cur.push_str(kind.suffix());
                lines.push(std::mem::take(&mut cur));
                pending_space = false;
            }
            Atom::Word(word) => {
                push(&mut lines, &mut cur, &mut pending_space, word, width);
            }
            Atom::Verbatim(block) => {
                let (first, rest) = block.split_first().expect("a verbatim leaf has lines");
                push(&mut lines, &mut cur, &mut pending_space, first, width);
                for line in rest {
                    lines.push(std::mem::take(&mut cur));
                    cur.push_str(line);
                }
            }
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Appends one unbreakable run, starting a new line when it does not fit.
fn push(
    lines: &mut Vec<String>,
    cur: &mut String,
    pending_space: &mut bool,
    word: &str,
    width: usize,
) {
    let w = display_width(word);
    let sep = usize::from(*pending_space);
    if !cur.is_empty() && display_width(cur) + sep + w > width {
        lines.push(std::mem::take(cur));
        *pending_space = false;
    }
    if *pending_space && !cur.is_empty() {
        cur.push(' ');
    }
    *pending_space = false;
    cur.push_str(word);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::fmt::inline::HardKind;

    fn atoms(text: &str) -> Vec<Atom> {
        let mut out = Vec::new();
        for (i, w) in text.split(' ').enumerate() {
            if i > 0 {
                out.push(Atom::Space);
            }
            out.push(Atom::Word(w.to_string()));
        }
        out
    }

    #[test]
    fn greedy_fill() {
        assert_eq!(wrap(&atoms("aaa bbb cc"), 7), ["aaa bbb", "cc"]);
        assert_eq!(wrap(&atoms("aaa bbb cc"), 6), ["aaa", "bbb cc"]);
        assert_eq!(wrap(&atoms("a b c"), UNLIMITED), ["a b c"]);
    }

    #[test]
    fn an_overlong_word_is_never_split() {
        let long = "https://example.com/very/long";
        assert_eq!(wrap(&atoms(&format!("x {long} y")), 8), ["x", long, "y"]);
    }

    #[test]
    fn no_trailing_spaces() {
        for line in wrap(&atoms("aaa bbbb cc dd"), 5) {
            assert_eq!(line, line.trim_end(), "{line:?}");
        }
    }

    #[test]
    fn hard_breaks_end_the_line_with_their_spelling() {
        let atoms = vec![
            Atom::Word("a".into()),
            Atom::HardBreak(HardKind::Spaces),
            Atom::Word("b".into()),
            Atom::HardBreak(HardKind::Backslash),
            Atom::Word("c".into()),
        ];
        assert_eq!(wrap(&atoms, 40), ["a  ", "b\\", "c"]);
    }

    #[test]
    fn a_verbatim_leaf_keeps_its_own_lines() {
        let atoms = vec![
            Atom::Word("a".into()),
            Atom::Space,
            Atom::Verbatim(vec!["`x".into(), "y`".into()]),
            Atom::Space,
            Atom::Word("b".into()),
        ];
        assert_eq!(wrap(&atoms, 40), ["a `x", "y` b"]);
    }
}
