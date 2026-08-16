//! The gap between a JavaScript pattern and a Rust one, closed where it is EXACT.
//!
//! Three rewrites, and the number is the point: everything else that differs —
//! `\cX`, the octal escapes, a variable-length lookbehind — is refused at
//! compilation, which is visible, where a wrong translation would be a regular
//! expression that quietly matches the wrong text. A rewrite belongs here only
//! when the JavaScript construct has exactly ONE meaning and Rust can spell it.
//!
//! Split out of [`super::compile`] when that file passed the crate's 500-line
//! ceiling. They are cohesive on their own terms: each is a pure
//! `&str -> String` over pattern SYNTAX, and none of them knows that an engine
//! exists.

/// `\/` back to `/`.
///
/// The one syntactic difference that is not optional. A literal has to escape
/// the slash that would otherwise end it, so `/a\/b/` is how a program spells
/// the pattern `a/b` — and both Rust engines reject `\/` as an unrecognised
/// escape, because neither has a delimiter to protect.
///
/// Only this one is translated. The rest of the gap between JavaScript's syntax
/// and Rust's — `\cX`, the octal escapes, `[]` meaning "match nothing" — is
/// named rather than papered over: a pattern using one is refused at
/// compilation, which is visible, where a wrong translation would be a regular
/// expression that matches the wrong text.
pub(super) fn unescape_solidus(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('/') => out.push('/'),
            // Any other escape is passed through unchanged, backslash included:
            // it is the engine's to interpret, and this function's whole job is
            // the one case where a JavaScript literal has a delimiter and a Rust
            // pattern does not.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Rewrites the two character classes JavaScript has and Rust's engine refuses.
///
/// `[]` matches NOTHING and `[^]` matches ANY character, both legal JavaScript
/// and both a parse error for the `regex` crate, which reads an empty class as
/// malformed. Neither is exotic: `[^]` is the ordinary way to write "any
/// character including a newline" in code predating the `s` flag, and it appears
/// in minified output constantly.
///
/// They are translated rather than refused because the translations are EXACT
/// and there is no ambiguity to lose: `[^\s\S]` is the empty set by
/// construction, and `[\s\S]` is its complement. That is the difference from
/// [`unescape_solidus`]'s neighbouring case, which refuses rather than guesses —
/// a wrong translation there would be a pattern matching the wrong text, and
/// here there is only one thing either class can mean.
///
/// Only outside a class, and only where the bracket is not itself escaped: `[[]`
/// is a class containing `[`, and `\[]` is the two literal characters. A blind
/// `replace` on the string got both wrong.
pub(super) fn empty_classes(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let characters: Vec<char> = pattern.chars().collect();
    let mut at = 0;
    let mut inside = false;
    while at < characters.len() {
        let character = characters[at];
        if character == '\\' {
            out.push(character);
            if let Some(next) = characters.get(at + 1) {
                out.push(*next);
            }
            at += 2;
            continue;
        }
        if !inside && character == '[' {
            if characters.get(at + 1) == Some(&']') {
                out.push_str("[^\\s\\S]");
                at += 2;
                continue;
            }
            if characters.get(at + 1) == Some(&'^') && characters.get(at + 2) == Some(&']') {
                out.push_str("[\\s\\S]");
                at += 3;
                continue;
            }
            inside = true;
        } else if inside && character == ']' {
            inside = false;
        }
        out.push(character);
        at += 1;
    }
    out
}

/// JavaScript's `.` is not Rust's `.`, and the difference is three characters.
///
/// Without `s`, the language says `.` matches everything except the four
/// **LineTerminator**s: `\n`, `\r`, U+2028 and U+2029. Both Rust engines exclude
/// `\n` alone, so `/a.b/.test("a\rb")` answered `true` here and `false`
/// everywhere else — a wrong answer rather than a refusal, which is the kind
/// this file's neighbouring comment says is worth translating for.
///
/// The translation is EXACT, which is what separates it from the escapes
/// [`unescape_solidus`] refuses to guess at: the class is the language's own
/// definition of the set written out.
///
/// Only outside a character class, where `.` is an ordinary member and means
/// itself, and never after a backslash. With `s` the caller does not call this
/// at all — `dot_matches_new_line` (and `(?s)` for the other engine) already
/// says the whole set is allowed.
pub(super) fn wide_dot(pattern: &str) -> String {
    const ANY: &str = "[^\\n\\r\\x{2028}\\x{2029}]";
    let mut out = String::with_capacity(pattern.len());
    let mut inside = false;
    let mut characters = pattern.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\' {
            out.push(character);
            if let Some(next) = characters.next() {
                out.push(next);
            }
            continue;
        }
        match character {
            '[' if !inside => inside = true,
            ']' if inside => inside = false,
            '.' if !inside => {
                out.push_str(ANY);
                continue;
            }
            _ => {}
        }
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dot_inside_a_class_is_an_ordinary_member() {
        assert_eq!(wide_dot("[.]"), "[.]");
        assert_eq!(wide_dot(r"\."), r"\.");
        assert!(wide_dot(".").starts_with("[^"));
    }

    #[test]
    fn the_two_empty_classes_are_translated_and_nothing_else_is() {
        assert_eq!(empty_classes("[]"), r"[^\s\S]");
        assert_eq!(empty_classes("[^]"), r"[\s\S]");
        assert_eq!(empty_classes("a[]b[^]c"), r"a[^\s\S]b[\s\S]c");
        // A bracket INSIDE a class is an ordinary member, and one that is
        // escaped is a literal — a blind `replace` got both of these wrong.
        assert_eq!(empty_classes("[[]"), "[[]");
        assert_eq!(empty_classes(r"\[]"), r"\[]");
        assert_eq!(empty_classes("[a]"), "[a]");
        assert_eq!(empty_classes("[^a]"), "[^a]");
        assert_eq!(empty_classes(r"[\]]"), r"[\]]");
    }

    #[test]
    fn an_escaped_slash_is_the_slash_a_literal_had_to_hide() {
        assert_eq!(unescape_solidus(r"a\/b"), "a/b");
        // Every other escape is the engine's to read, backslash included.
        assert_eq!(unescape_solidus(r"a\wb"), r"a\wb");
    }
}

