//! Annex B's quantifiable lookahead, rewritten so the backtracking engine takes it.
//!
//! # The gap
//!
//! In a pattern without the `u`/`v` flag, JavaScript's grammar lets a quantifier
//! apply to a lookahead: `QuantifiableAssertion :: (?= Disjunction) | (?!
//! Disjunction)`, so `/(?=lg)?x/i` is a valid expression — WhatsApp Web's
//! device-detection sniffer spells exactly that. `fancy-regex` refuses it
//! ("Target of repeat operator is invalid"), and so the whole literal threw a
//! `SyntaxError` and the page's bootstrap died on it. `regex` refuses every
//! lookaround anyway, so closing this is purely about what the fallback engine
//! is handed.
//!
//! # Why a rewrite and not an engine change
//!
//! The quantifier on a zero-width assertion has an exact meaning the engine can
//! already spell, so this belongs beside `translate`'s other EXACT rewrites
//! rather than in the matcher. A lookahead consumes nothing, so repeating it is
//! not like repeating text:
//!
//! - `(?=X)?`, `(?=X)*`, `(?=X){0,m}` — match the assertion zero or one time;
//!   either way the position does not move. That is "the assertion, optionally",
//!   which is `(?:(?=X)|)` — the lookahead OR nothing — and `fancy-regex` takes
//!   it. The greedy quantifier prefers the assertion, so it leads the
//!   alternation; a lazy one (`??`, `*?`) prefers skipping, so the empty arm
//!   leads: `(?:|(?=X))`.
//! - `(?=X)+`, `(?=X){1,}`, `(?=X){n}` with n ≥ 1 — match it one or more times,
//!   and a zero-width assertion matched again at the same position adds nothing,
//!   so this is just `(?=X)`. The quantifier is dropped.
//!
//! Only in Annex B. With `u`/`v` the same syntax is a `SyntaxError` in the
//! language, so this does not run and the pattern is left to be refused — which
//! is the right answer there, not a rewrite.
//!
//! # What is deliberately left alone
//!
//! A quantified LOOKBEHIND (`(?<=X)?`) is NOT a `QuantifiableAssertion` even in
//! Annex B — the grammar admits only the two lookaheads — so a program that
//! writes one is writing an error, and leaving it to be refused keeps this rule
//! matched to the language rather than more permissive than it.

/// Rewrites every quantified lookahead in `pattern` into a form `fancy-regex`
/// accepts, leaving the rest untouched. For Annex B only; see the module header.
pub(in crate::entry::regex) fn quantifiable_assertions(pattern: &str) -> String {
    // The common pattern has no lookahead at all, and the scan below allocates a
    // `Vec<char>`; skip both when there is plainly nothing to rewrite.
    if !pattern.contains("(?") {
        return pattern.to_owned();
    }
    let chars: Vec<char> = pattern.chars().collect();
    rewrite(&chars)
}

/// The scan, over a slice so the recursion for a nested lookahead costs no
/// re-collection.
fn rewrite(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    let mut in_class = false;
    while i < chars.len() {
        let c = chars[i];
        // An escape carries its next character verbatim, inside a class or out —
        // `\(` is not a group and `\]` does not end a class.
        if c == '\\' {
            out.push(c);
            if i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if in_class {
            out.push(c);
            if c == ']' {
                in_class = false;
            }
            i += 1;
            continue;
        }
        if c == '[' {
            in_class = true;
            out.push(c);
            i += 1;
            continue;
        }
        // A lookahead opens with `(?=` or `(?!`. A lookbehind (`(?<`), a named
        // group (`(?<name>`) and a non-capturing group (`(?:`) all begin
        // differently and are walked through as ordinary text below.
        if c == '('
            && i + 2 < chars.len()
            && chars[i + 1] == '?'
            && (chars[i + 2] == '=' || chars[i + 2] == '!')
        {
            if let Some(close) = matching_paren(chars, i) {
                let quant = parse_quantifier(chars, close + 1);
                // The content is rewritten in turn, so a quantified lookahead
                // nested inside this one is handled as well.
                let inner = rewrite(&chars[i + 3..close]);
                let assertion = format!("(?{}{})", chars[i + 2], inner);
                match quant {
                    Some(Quantifier { end, zero_min, lazy }) => {
                        match (zero_min, lazy) {
                            (true, false) => {
                                out.push_str("(?:");
                                out.push_str(&assertion);
                                out.push_str("|)");
                            }
                            (true, true) => {
                                out.push_str("(?:|");
                                out.push_str(&assertion);
                                out.push(')');
                            }
                            // One or more times is once, for a zero-width thing.
                            (false, _) => out.push_str(&assertion),
                        }
                        i = end;
                        continue;
                    }
                    None => {
                        // No quantifier: emit the assertion with its content
                        // already rewritten, and carry on after it.
                        out.push_str(&assertion);
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// A quantifier that followed a lookahead.
struct Quantifier {
    /// The index just past the quantifier, where scanning resumes.
    end: usize,
    /// Whether it admits zero repetitions (`?`, `*`, `{0,…}`): optional rather
    /// than mandatory.
    zero_min: bool,
    /// Whether it is lazy (a trailing `?`), which decides which arm of the
    /// optional alternation leads.
    lazy: bool,
}

/// Reads a quantifier at `idx`, or `None` when nothing there is one.
fn parse_quantifier(chars: &[char], idx: usize) -> Option<Quantifier> {
    let first = *chars.get(idx)?;
    let (mut end, zero_min) = match first {
        '?' | '*' => (idx + 1, true),
        '+' => (idx + 1, false),
        '{' => {
            // `{n}`, `{n,}` or `{n,m}` — anything else is a literal brace, which
            // is not a quantifier and must not be rewritten.
            let (close, min) = brace_quantifier(chars, idx)?;
            (close + 1, min == 0)
        }
        _ => return None,
    };
    // A trailing `?` makes it lazy; it changes nothing about how many times a
    // zero-width assertion matches, only which arm the alternation prefers.
    let lazy = chars.get(end) == Some(&'?');
    if lazy {
        end += 1;
    }
    Some(Quantifier { end, zero_min, lazy })
}

/// Parses `{n}` / `{n,}` / `{n,m}` at `idx` (which is `{`), returning the index
/// of the closing `}` and the lower bound `n`. `None` for anything that is not a
/// well-formed counted quantifier — a bare `{` is an ordinary character.
fn brace_quantifier(chars: &[char], idx: usize) -> Option<(usize, u32)> {
    let mut j = idx + 1;
    let start = j;
    while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
        j += 1;
    }
    if j == start {
        return None; // `{,` or `{}` — no lower bound, not a quantifier
    }
    let min: u32 = chars[start..j].iter().collect::<String>().parse().ok()?;
    match chars.get(j) {
        Some('}') => Some((j, min)),
        Some(',') => {
            // `{n,}` or `{n,m}`: skip optional upper bound to the `}`.
            j += 1;
            while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
            }
            match chars.get(j) {
                Some('}') => Some((j, min)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The index of the `)` that closes the group opening at `open`, aware of
/// escapes and character classes (where a `)` is a literal). `None` if the group
/// is never closed.
fn matching_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    let mut in_class = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            i += 2;
            continue;
        }
        if in_class {
            if c == ']' {
                in_class = false;
            }
            i += 1;
            continue;
        }
        match c {
            '[' => in_class = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::quantifiable_assertions as q;

    #[test]
    fn an_optional_lookahead_becomes_the_assertion_or_nothing() {
        // `(?=lg)?` is "optionally assert lg follows" — a no-op zero-width thing
        // the backtracking engine spells as the lookahead OR empty.
        assert_eq!(q("(?=lg)?x"), "(?:(?=lg)|)x");
        assert_eq!(q("(?!lg)?x"), "(?:(?!lg)|)x");
        assert_eq!(q("(?=lg)*x"), "(?:(?=lg)|)x");
        assert_eq!(q("(?=lg){0,3}x"), "(?:(?=lg)|)x");
    }

    #[test]
    fn a_lazy_optional_lookahead_leads_with_the_empty_arm() {
        assert_eq!(q("(?=lg)??x"), "(?:|(?=lg))x");
        assert_eq!(q("(?=lg)*?x"), "(?:|(?=lg))x");
    }

    #[test]
    fn a_mandatory_repeat_of_a_zero_width_assertion_is_just_the_assertion() {
        // Matching a lookahead one-or-more times at one position is matching it
        // once, so the quantifier is dropped.
        assert_eq!(q("(?=lg)+x"), "(?=lg)x");
        assert_eq!(q("(?=lg){1,}x"), "(?=lg)x");
        assert_eq!(q("(?=lg){2}x"), "(?=lg)x");
    }

    #[test]
    fn a_lookahead_without_a_quantifier_is_untouched() {
        assert_eq!(q("(?=lg)x"), "(?=lg)x");
        assert_eq!(q("a(?!x)b"), "a(?!x)b");
    }

    #[test]
    fn the_whatsapp_pattern_is_accepted_after_rewriting() {
        let rewritten = q("((?=lg)?[vl]k\\-?\\d{3}) bui");
        assert_eq!(rewritten, "((?:(?=lg)|)[vl]k\\-?\\d{3}) bui");
        assert!(
            fancy_regex::Regex::new(&rewritten).is_ok(),
            "the rewrite has to be something the backtracking engine takes"
        );
    }

    #[test]
    fn a_quantifier_on_ordinary_text_is_left_alone() {
        // Nothing here is a lookahead, so the `?`/`+`/`{2}` stay exactly as
        // written — this rule touches assertions and nothing else.
        assert_eq!(q("ab?c+d{2}"), "ab?c+d{2}");
        assert_eq!(q("(ab)?c"), "(ab)?c");
    }

    #[test]
    fn a_quantified_lookahead_inside_a_class_is_a_literal() {
        // Inside `[...]`, `(?=lg)?` is a set of characters, not an assertion.
        assert_eq!(q("[(?=lg)]?x"), "[(?=lg)]?x");
    }

    #[test]
    fn a_nested_quantified_lookahead_is_rewritten_too() {
        assert_eq!(q("(?=a(?=b)?c)x"), "(?=a(?:(?=b)|)c)x");
    }
}
