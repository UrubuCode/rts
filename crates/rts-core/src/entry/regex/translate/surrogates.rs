//! A surrogate range inside a character class — #2837.
//!
//! # The rule this breaks, and why it was agreed
//!
//! `translate.rs` says a rewrite belongs there only when the JavaScript construct
//! has exactly ONE meaning and Rust can spell it, because a regular expression
//! that quietly matches the wrong text is worse than a visible refusal. **This
//! rewrite does not meet that bar**, and it is here because the refusal it
//! replaces is not visible in the way that rule assumes.
//!
//! The refusal happens when the PATTERN is compiled, so it takes the whole
//! program with it — including every use of that pattern that would have agreed.
//! And the uses that agree are the overwhelming majority: the pattern this was
//! found through is
//!
//! ```text
//! /[\u0000-\u001f"\\ud800-\udfff]/
//! ```
//!
//! which `safe-stable-stringify` uses in `.test(str)` and nothing else — as a fast
//! path, where a match means "hand this to `JSON.stringify`". There, the rewrite is
//! exact, and even a false positive would be harmless.
//!
//! Agreed 2026-10-02, with the divergences written down rather than discovered.
//!
//! # What it does
//!
//! `\ud800-\udfff` becomes `\u{10000}-\u{10FFFF}`, and only when the pattern has
//! NO `u` flag. Without that flag a JavaScript class matches UTF-16 CODE UNITS, so
//! a surrogate range matches either half of a surrogate pair — and in well-formed
//! text, a half of a pair appears exactly where an astral character does. Rust
//! matches code points, where that same character is one element.
//!
//! # What agrees and what does not, measured under node 22.23.2
//!
//! | asked of `"a😀b"` | JavaScript | here |
//! |---|---|---|
//! | `test` | `true` | `true` |
//! | `exec().index` | 1 | 1 |
//! | `exec()[0].length` | 1 | **2** |
//! | matches with `g` | 2 | **1** |
//! | `replace(re, "#")` | `"a##b"` | `"a#b"` |
//! | `test` of a LONE surrogate | `true` | **`false`** |
//!
//! So: a predicate agrees, a position agrees, and anything that measures or
//! consumes the match does not. A program that escapes text by replacing through
//! this class gets one replacement where JavaScript makes two — which for the JSON
//! case produces a different escape, not a corrupt one, because the replacement
//! sees a whole character.
//!
//! # Why the `u` flag is excluded, and it is not caution
//!
//! With `u` the same class means something else entirely, and node was measured
//! saying so: `/[\ud800-\udfff]/u.test("a😀b")` is **`false`**, where the same
//! pattern without `u` is `true`. Under `u` the class is a range of surrogate code
//! points, which only a lone surrogate can be. Rewriting it to the astral range
//! would turn a `false` into a `true` — inverting an answer rather than widening
//! one — so a `u` pattern is left exactly as it was, and still refused.

/// The first and last code unit of the surrogate block.
const FIRST: u32 = 0xD800;
const LAST: u32 = 0xDFFF;

/// Rewrites surrogate ranges inside character classes.
///
/// `unicode` is the `u`/`v` flag; when it is set the pattern comes back
/// unchanged, for the reason the module doc gives.
pub(in crate::entry::regex) fn astral_surrogate_classes(pattern: &str, unicode: bool) -> String {
    if unicode || !pattern.contains("\\u") {
        return pattern.to_owned();
    }
    let characters: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len());
    let mut at = 0;
    let mut inside_class = false;
    while at < characters.len() {
        let character = characters[at];
        // An escape is copied with whatever follows it, so a `\[` never opens a
        // class and a `\\` never swallows the next character's meaning.
        if character == '\\' {
            if let Some(rewritten) = range_at(&characters, at) {
                let (text, next) = rewritten;
                // Only inside a class: `\ud800-\udfff` outside one is three
                // things in a row (an escape, a literal dash, an escape), not a
                // range, and rewriting it there would change what it matches.
                match inside_class {
                    true => out.push_str(&text),
                    false => out.extend(&characters[at..next]),
                }
                at = next;
                continue;
            }
            out.push(character);
            if let Some(following) = characters.get(at + 1) {
                out.push(*following);
            }
            at += 2;
            continue;
        }
        match character {
            '[' if !inside_class => inside_class = true,
            ']' if inside_class => inside_class = false,
            _ => {}
        }
        out.push(character);
        at += 1;
    }
    out
}

/// A `\uXXXX-\uXXXX` range at `at`, rewritten, and where it ends.
///
/// `None` when what is at `at` is not such a range, including when it is one
/// whose bounds are not both inside the surrogate block — a range that only
/// touches it is left alone rather than approximated, because splitting it into
/// the part that maps and the part that does not is a second meaning this is not
/// agreed to invent.
fn range_at(characters: &[char], at: usize) -> Option<(String, usize)> {
    let (low, after_low) = escape_at(characters, at)?;
    if characters.get(after_low) != Some(&'-') {
        return None;
    }
    let (high, after_high) = escape_at(characters, after_low + 1)?;
    let covers = (FIRST..=LAST).contains(&low) && (FIRST..=LAST).contains(&high) && low <= high;
    match covers {
        true => Some(("\\x{10000}-\\x{10FFFF}".to_owned(), after_high)),
        false => None,
    }
}

/// One `\uXXXX` at `at`: its value, and where it ends.
fn escape_at(characters: &[char], at: usize) -> Option<(u32, usize)> {
    if characters.get(at) != Some(&'\\') || characters.get(at + 1) != Some(&'u') {
        return None;
    }
    let digits: String = characters.get(at + 2..at + 6)?.iter().collect();
    if digits.len() != 4 || !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    Some((u32::from_str_radix(&digits, 16).ok()?, at + 6))
}

#[cfg(test)]
mod tests {
    use super::astral_surrogate_classes as rewrite;

    /// The pattern this was found through, and the shape every JSON escaper uses.
    #[test]
    fn the_json_escape_class_becomes_one_rust_accepts() {
        let answered = rewrite("[\\u0000-\\u001f\\u0022\\u005c\\ud800-\\udfff]", false);
        assert_eq!(answered, "[\\u0000-\\u001f\\u0022\\u005c\\x{10000}-\\x{10FFFF}]");
    }

    /// A `u` pattern is left alone, because the class means the opposite there —
    /// node answers `false` for astral text under `u` and `true` without it.
    #[test]
    fn the_unicode_flag_is_left_exactly_as_it_was() {
        let written = "[\\ud800-\\udfff]";
        assert_eq!(rewrite(written, true), written);
    }

    /// Outside a class the same three tokens are not a range.
    #[test]
    fn outside_a_class_nothing_is_rewritten() {
        let written = "\\ud800-\\udfff";
        assert_eq!(rewrite(written, false), written);
    }

    /// A range that only TOUCHES the block is left alone rather than split.
    #[test]
    fn a_range_crossing_the_block_is_not_approximated() {
        let written = "[\\u0000-\\udfff]";
        assert_eq!(rewrite(written, false), written);
        let other = "[\\ud800-\\uffff]";
        assert_eq!(rewrite(other, false), other);
    }

    /// An escaped bracket does not open a class, so what follows is not inside one.
    #[test]
    fn an_escaped_bracket_does_not_open_a_class() {
        let written = "\\[\\ud800-\\udfff";
        assert_eq!(rewrite(written, false), written);
    }

    /// A pattern with no `\\u` at all is returned without being walked.
    #[test]
    fn a_pattern_without_escapes_is_untouched() {
        assert_eq!(rewrite("[a-z]+", false), "[a-z]+");
    }
}
