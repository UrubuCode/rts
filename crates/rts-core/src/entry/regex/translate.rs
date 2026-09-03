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


/// The class operators Rust's engine has and JavaScript does not.
///
/// Inside `[...]`, JavaScript gives `[`, `&` and `~` no meaning at all: each is
/// an ordinary member of the set, needing no escape. Rust's `regex` reads the
/// first as a NESTED class and the other two, doubled, as set operators —
/// `&&` intersection and `~~` symmetric difference. So the same three
/// characters spell two different sets in the two languages, and `\\[`, `\\&`
/// and `\\~` are how Rust writes what JavaScript meant.
///
/// # Why this belongs here and is not a refusal
///
/// The bar this file sets is that the JavaScript construct has exactly ONE
/// meaning and Rust can spell it. A bracket inside a class is that, with no
/// ambiguity to lose: outside the `v` flag — which this engine does not
/// implement, and whose whole point is to GIVE these characters meaning — the
/// specification says a `ClassAtom` may be any `SourceCharacter` but `]`,
/// `\\` and `-`. Escaping cannot change which set is matched; it only stops
/// Rust from reading an operator that was never written.
///
/// # The two failures this closes, and only one of them was visible
///
/// `/[a[b]/` was REFUSED — a `SyntaxError` at compile, reported as issue
/// #2612 because `get-intrinsic` (a transitive dependency of hundreds of npm
/// packages) carries `/[^%.[\\]]+|.../` and threw on load.
///
/// `/[a&&b]/` was worse and nobody had reported it: it COMPILED, as the
/// intersection of `{a}` and `{b}`, which is empty — so `/[a&&b]/.test("a")`
/// answered `false` where every other engine answers `true`. A wrong answer
/// with no error is the failure this crate's own doc calls the one worth
/// paying attention to, and it was reachable from a pattern nobody would
/// think twice about writing.
///
/// # What is NOT translated, and why each is left
///
/// `-` is not escaped. It is a range in both languages, so escaping every one
/// of them would destroy `[a-z]`, and the only divergence is the DOUBLED
/// `[a--b]`: Rust reads set difference where JavaScript reads the range `a` to
/// `-` and rejects it as out of order. That is a pattern JavaScript refuses,
/// so nothing correct depends on it — this engine accepting it is a laxity,
/// not a wrong answer, and narrowing it would need the range parser this file
/// deliberately does not have.
///
/// A backslash and what follows it pass through untouched, so a pattern that
/// already escaped one of the three is not escaped twice.
pub(super) fn class_operators(pattern: &str) -> String {
    // The six escapes that stand for a SET rather than a character. A dash
    // beside one of them cannot be a range — a range needs two single
    // characters — so JavaScript reads it as the dash itself, while Rust
    // refuses the pattern outright: `/[\w-\d]/` was a SyntaxError here and
    // matches a dash, a word character or a digit in Node.
    const CLASS_ESCAPE: &str = "dDwWsS";
    let mut out = String::with_capacity(pattern.len());
    let mut inside = false;
    // Whether the atom just emitted was one of the six above, which is what
    // makes a following dash a literal instead of the start of a range.
    let mut after_class_escape = false;
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            out.push(character);
            if let Some(next) = characters.next() {
                out.push(next);
                after_class_escape = inside && CLASS_ESCAPE.contains(next);
                continue;
            }
            continue;
        }
        match character {
            // The bracket that OPENS a class, and the one that closes it. A
            // class cannot nest in JavaScript, so the first `[` is the opener
            // and every later one is a member.
            '[' if !inside => inside = true,
            ']' if inside => inside = false,
            '[' | '&' | '~' if inside => {
                out.push('\\');
            }
            // A dash with a set on either side. Looking ahead as well as
            // behind, because `[\d-z]` and `[a-\d]` are both refused by Rust
            // and both ordinary in JavaScript.
            '-' if inside && (after_class_escape || next_is_class_escape(&characters, CLASS_ESCAPE)) => {
                out.push('\\');
            }
            _ => {}
        }
        after_class_escape = false;
        out.push(character);
    }
    out
}

/// Whether what follows is one of the set escapes, without consuming it.
///
/// Split out so [`class_operators`] reads as one rule per line; a peekable
/// iterator would have to be threaded through every arm of that match for
/// the sake of this one lookahead.
fn next_is_class_escape(rest: &std::str::Chars<'_>, set: &str) -> bool {
    let mut ahead = rest.clone();
    ahead.next() == Some('\\') && ahead.next().is_some_and(|letter| set.contains(letter))
}


/// The letters JavaScript spells with a backslash and Rust reads as syntax.
///
/// Outside the `u` flag, JavaScript's `IdentityEscape` says a backslash before
/// a character it does not recognise IS that character: `/\A/` matches the
/// letter `A`, `/\z/` the letter `z`, `/\q/` the letter `q`. Measured against
/// Node v20 rather than read off the grammar — all 32 of the letters below
/// answer `true` to `new RegExp("\\" + letter).test(letter)`, and NONE of them
/// answers anything else.
///
/// Rust's engine spends several of them: `\A` is start-of-text, `\z` and `\Z`
/// end-of-text, `\q{...}` a literal string, `\p{...}` a Unicode property. So
/// the same two characters are a letter in one language and an anchor in the
/// other, and the backslash has to go.
///
/// # What each of the three groups was doing before
///
/// `\A`, `\z`: COMPILED, as anchors, so `/\A/.test("a")` answered `true`
/// where Node answers `false` — a wrong answer with no error, the same shape
/// as `[a&&b]` one function up.
///
/// `\Z`, `\q{ab}`, `\k<n>` with no named group: REFUSED, a `SyntaxError` on a
/// pattern every other engine accepts.
///
/// `\p{L}` and `\P{L}` are the pair that DEPENDS on the flag, which is why
/// this function takes it. With `u` they are Unicode property escapes in both
/// languages and are left alone; without it JavaScript reads `p` and `{L}`
/// separately — `/[\p{L}]/.test("a")` is `false` in Node, because the class
/// holds `p`, `{`, `L`, `}` — while Rust reads the property either way and
/// answered `true`.
///
/// # What is NOT translated here
///
/// `\cX` keeps the refusal [`unescape_solidus`]'s doc already names: it is a
/// control escape in JavaScript with no Rust spelling that is exact for every
/// `X`, and guessing is the thing this file exists not to do.
///
/// A digit escape is left for the same reason from the other side: `\1` is a
/// backreference in both, and `\0` is NUL in JavaScript but not something
/// Rust's parser takes inside a class. Both are named in the module doc rather
/// than rewritten, because a backreference renumbered by a rewrite is a silent
/// wrong answer of exactly the kind this function was written to remove.
pub(super) fn identity_escapes(pattern: &str, unicode: bool) -> String {
    // The 32 letters measured against Node: every one is itself, and none of
    // them means anything else. Written out rather than computed as "not in
    // the recognised set", so that adding a recognised escape later cannot
    // silently start rewriting it.
    const LITERAL: &str = "ACEFGHIJKLMNOQRTUVXYZaeghijlmoqyz";
    // A named backreference is only a backreference when the pattern declares
    // a named group; without one, Annex B reads `\k` as the letter.
    let named_groups = pattern.contains("(?<")
        && !pattern.contains("(?<=")
        && !pattern.contains("(?<!")
        || pattern.contains("(?<") && pattern.matches("(?<").count() > pattern.matches("(?<=").count() + pattern.matches("(?<!").count();
    let mut out = String::with_capacity(pattern.len());
    let mut characters = pattern.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some(next) if LITERAL.contains(next) => out.push(next),
            // The flag decides these two, and only these two.
            Some(next @ ('p' | 'P')) if !unicode => out.push(next),
            Some('k') if !named_groups => out.push('k'),
            // `\0` is NUL in JavaScript. Rust's parser takes it outside a class
            // and REFUSES it inside one, so `/[\0]/` was a SyntaxError on a
            // pattern Node compiles. `\x00` is the same character, spelled the
            // way both parsers read, and it is only substituted when no digit
            // follows: `\0` then `1` is a legacy octal escape, which this file
            // refuses rather than guesses at.
            Some('0') if !characters.clone().next().is_some_and(|d| d.is_ascii_digit()) => {
                out.push_str("\\x00");
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
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

    #[test]
    fn a_bracket_inside_a_class_is_an_ordinary_member() {
        // Issue #2612: refused before this rewrite existed.
        assert_eq!(class_operators("[a[b]"), r"[a\[b]");
        assert_eq!(class_operators("[^a[b]"), r"[^a\[b]");
        assert_eq!(class_operators("[[]"), r"[\[]");
        // The pattern from `get-intrinsic`, which is what made this reachable
        // from hundreds of npm packages.
        assert_eq!(class_operators(r"[^%.[\]]+"), r"[^%.\[\]]+");
    }

    #[test]
    fn the_set_operators_rust_has_and_javascript_does_not() {
        // `&&` compiled as INTERSECTION and answered the empty set, so
        // `/[a&&b]/.test("a")` was `false` where every other engine says true.
        assert_eq!(class_operators("[a&&b]"), r"[a\&\&b]");
        assert_eq!(class_operators("[a~~b]"), r"[a\~\~b]");
        // Single ones are literals in both, and escaping them is still right:
        // it costs nothing and keeps one rule instead of two.
        assert_eq!(class_operators("[a&b]"), r"[a\&b]");
        assert_eq!(class_operators("[a~b]"), r"[a\~b]");
    }

    #[test]
    fn outside_a_class_none_of_the_three_is_touched() {
        assert_eq!(class_operators("a&&b"), "a&&b");
        assert_eq!(class_operators("a~b"), "a~b");
        // A bracket outside a class OPENS one; it is not a member.
        assert_eq!(class_operators("a[b]c"), "a[b]c");
        assert_eq!(class_operators("(a|b)"), "(a|b)");
    }

    #[test]
    fn an_already_escaped_member_is_not_escaped_twice() {
        assert_eq!(class_operators(r"[a\[b]"), r"[a\[b]");
        assert_eq!(class_operators(r"[a\&b]"), r"[a\&b]");
        // `\]` does not close the class, so what follows is still inside it.
        assert_eq!(class_operators(r"[a\]&b]"), r"[a\]\&b]");
        // A trailing backslash is passed through rather than read past the end.
        assert_eq!(class_operators(r"[a\"), r"[a\");
    }

    #[test]
    fn a_range_is_left_alone_because_the_dash_means_the_same_thing() {
        assert_eq!(class_operators("[a-z]"), "[a-z]");
        assert_eq!(class_operators("[a-z0-9_]"), "[a-z0-9_]");
        // The doubled dash is the one divergence deliberately left: JavaScript
        // rejects it, so nothing correct depends on either reading.
        assert_eq!(class_operators("[a--b]"), "[a--b]");
    }

    #[test]
    fn a_backslash_before_a_letter_rust_spends_is_that_letter() {
        // Compiled as ANCHORS before this existed, so `/\A/.test("a")` was true.
        assert_eq!(identity_escapes(r"\A", false), "A");
        assert_eq!(identity_escapes(r"\z", false), "z");
        // Refused before this existed.
        assert_eq!(identity_escapes(r"\Z", false), "Z");
        assert_eq!(identity_escapes(r"\q{ab}", false), "q{ab}");
        assert_eq!(identity_escapes(r"[\q{ab}]", false), "[q{ab}]");
    }

    #[test]
    fn the_escapes_javascript_does_recognise_are_left_alone() {
        for kept in [r"\d", r"\D", r"\w", r"\W", r"\s", r"\S", r"\b", r"\B", r"\n", r"\r", r"\t", r"\v", r"\f"] {
            assert_eq!(identity_escapes(kept, false), kept, "{kept} must not be rewritten");
        }
        // A backreference keeps its number: renumbering one would be exactly
        // the silent wrong answer this function exists to remove.
        assert_eq!(identity_escapes(r"(a)\1", false), r"(a)\1");
        // And an escaped metacharacter stays escaped.
        assert_eq!(identity_escapes(r"\.\*\[\]\\", false), r"\.\*\[\]\\");
        // `\cX` keeps its existing refusal rather than being guessed at.
        assert_eq!(identity_escapes(r"\cA", false), r"\cA");
    }

    #[test]
    fn the_property_escape_is_the_one_the_flag_decides() {
        // Without `u`, Node reads `p` `{` `L` `}` — `/[\p{L}]/.test("a")` is false.
        assert_eq!(identity_escapes(r"[\p{L}]", false), "[p{L}]");
        assert_eq!(identity_escapes(r"\P{L}", false), "P{L}");
        // With `u` it is a property escape in both languages.
        assert_eq!(identity_escapes(r"[\p{L}]", true), r"[\p{L}]");
        assert_eq!(identity_escapes(r"\p{Script=Greek}", true), r"\p{Script=Greek}");
    }

    #[test]
    fn a_named_backreference_survives_only_when_a_named_group_exists() {
        assert_eq!(identity_escapes(r"(?<n>a)\k<n>", false), r"(?<n>a)\k<n>");
        // No named group: Annex B reads the letter, and Rust refused the pattern.
        assert_eq!(identity_escapes(r"\k<n>", false), "k<n>");
        // A lookbehind is not a named group, and it opens with the same three
        // characters — the reason this asks more than `contains("(?<")`.
        assert_eq!(identity_escapes(r"(?<=a)\k<n>", false), "(?<=a)k<n>");
        assert_eq!(identity_escapes(r"(?<!a)\k<n>", false), "(?<!a)k<n>");
    }

    #[test]
    fn a_dash_beside_a_set_escape_is_a_literal_dash() {
        // Refused by Rust, ordinary in JavaScript: a range needs two single
        // characters, so a dash next to `\d` can only be itself.
        assert_eq!(class_operators(r"[\w-\d]"), r"[\w\-\d]");
        assert_eq!(class_operators(r"[\d-z]"), r"[\d\-z]");
        assert_eq!(class_operators(r"[a-\d]"), r"[a\-\d]");
        // A real range keeps its dash, which is the whole reason this asks
        // what is on either side instead of escaping every dash.
        assert_eq!(class_operators("[a-z]"), "[a-z]");
        assert_eq!(class_operators("[a-z0-9]"), "[a-z0-9]");
        // And outside a class a dash was never anything but itself.
        assert_eq!(class_operators(r"a-\d"), r"a-\d");
    }

    #[test]
    fn the_nul_escape_is_spelled_the_way_both_parsers_read() {
        // `/[\0]/` was a SyntaxError; Node matches the NUL character.
        assert_eq!(identity_escapes(r"[\0]", false), r"[\x00]");
        assert_eq!(identity_escapes(r"\0", false), r"\x00");
        // A digit after it is a legacy octal escape, which is refused rather
        // than guessed at — so it is left exactly as written.
        assert_eq!(identity_escapes(r"\01", false), r"\01");
    }


}

