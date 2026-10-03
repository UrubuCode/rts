//! Splitting a LEADING lookbehind off a pattern, so the runtime can check it
//! itself.
//!
//! # Why this exists at all
//!
//! JavaScript is one of the few languages whose lookbehind may be of unbounded
//! width: `/(?<=\.\s*)[a-z]+/` is ordinary and `"a. foo".match` of it answers
//! `["foo"]` in both Node and Bun. `fancy-regex`, the backtracking half of
//! [`super::compile::Engine`], compiles a lookbehind as `GoBack(n)` for a fixed
//! `n` and refuses anything else with `CompileError::LookBehindNotConst` — so
//! the whole pattern was refused and `new RegExp` threw a `SyntaxError` at a
//! pattern every other engine runs.
//!
//! The width is the only thing it cannot do: an ALTERNATION of differently
//! sized branches is refused for the same reason, while `(?<=ab|c)d` happens to
//! survive because `regex` is asked first and the branches there are literals.
//! That is why the restriction reads as "no quantifier" from the outside.
//!
//! # Why a split rather than a rewrite
//!
//! [`super::translate`] rewrites a pattern into one Rust can read, and that was
//! the first thing tried here: expand the quantifier into an alternation of
//! fixed-width lookbehinds, `(?<=x{1,3})` into `(?:(?<=x)|(?<=xx)|(?<=xxx))`.
//! It is exact for a BOUNDED quantifier and impossible for `*`, `+` and
//! `{n,}` — which is the case that matters, and the one the issue was raised
//! for. Capping the expansion would answer a wrong result for a long enough
//! subject, and #2839 settled that a pattern with no spelling in Rust is
//! REFUSED rather than approximated.
//!
//! So instead the lookbehind is taken out of the pattern and the runtime
//! answers it, by asking a second compiled pattern whether the lookbehind's
//! body ends exactly where the match would begin. That question —
//! `(?:inner)\z` against `&subject[..at]` — is exact and has no width limit,
//! because the engine is free to start it anywhere in the prefix.
//!
//! # What that costs, and the price of admission
//!
//! It works only where the lookbehind is the FIRST thing in the pattern, since
//! that is the only position whose candidates the runtime can enumerate: the
//! overall match begins exactly where the body's match begins, so a candidate
//! that fails the check is skipped by advancing one character. A lookbehind in
//! the middle of a pattern is a question about a position only the engine knows,
//! and nothing here can reach it.
//!
//! And the lookbehind's own content must capture nothing. That is not a
//! convenience: JavaScript matches a lookbehind RIGHT TO LEFT, so a group inside
//! one captures a different substring than the same group read forwards
//! (issue #2502). Asking for a BOOLEAN makes the direction unobservable, which
//! is what makes this exact rather than nearly right. A capture inside is
//! refused by name.

/// A pattern whose leading lookbehind the runtime will check itself.
pub(super) struct Split {
    /// What must end where the match begins — the lookbehind's own content,
    /// with the surrounding `(?<=` and `)` removed.
    pub(super) inner: String,
    /// `(?<!` rather than `(?<=`.
    pub(super) negated: bool,
    /// Everything after the lookbehind: the pattern that is actually matched,
    /// and whose group numbering is therefore the whole pattern's.
    pub(super) body: String,
}

/// Why a pattern with a lookbehind in it cannot be run this way.
///
/// Each one becomes a sentence in the `SyntaxError`, because
/// `invalid regular expression: /<the whole pattern>/` is the message that told
/// nobody anything — the reason it was refused is the only part a reader cannot
/// work out from the pattern they just wrote.
pub(super) enum Reason {
    /// There is a lookbehind, but not at the start of the pattern.
    NotLeading,
    /// `(?<=` with no closing parenthesis.
    Unterminated,
    /// A capture group inside the lookbehind — see this module's last page.
    Captures,
    /// A lookahead inside the lookbehind. It would look past the position the
    /// check is made at, and the check is given the text BEFORE it only.
    LooksAhead,
    /// `$` inside the lookbehind. It would mean "the end of the prefix" here
    /// and means "the end of the subject" in the language.
    EndAnchor,
    /// `\b` or `\B` inside the lookbehind. A word boundary reads the character
    /// on BOTH sides, and the one after the position is not in the prefix.
    WordBoundary,
    /// A backreference inside the lookbehind. Every group it could name is in
    /// the body, which the check never sees.
    Backreference,
}

impl Reason {
    /// What the `SyntaxError` says after the pattern.
    pub(super) fn message(&self) -> &'static str {
        match self {
            Reason::NotLeading => {
                "a lookbehind of unbounded width is supported only as the first thing in the pattern"
            }
            Reason::Unterminated => "the lookbehind has no closing parenthesis",
            Reason::Captures => {
                "a capture group inside a lookbehind of unbounded width is not supported"
            }
            Reason::LooksAhead => "a lookahead inside a lookbehind is not supported",
            Reason::EndAnchor => "`$` inside a lookbehind is not supported",
            Reason::WordBoundary => "`\\b` inside a lookbehind is not supported",
            Reason::Backreference => "a backreference inside a lookbehind is not supported",
        }
    }
}

/// Takes a leading lookbehind off `pattern`, or says why it cannot.
///
/// `Err(None)` is a pattern with no lookbehind in it anywhere: whatever made
/// the engines refuse it is not this module's business, and the caller keeps
/// its own message.
pub(super) fn split(pattern: &str) -> Result<Split, Option<Reason>> {
    let Some(negated) = opens_lookbehind(pattern) else {
        return Err(match finds_lookbehind(pattern) {
            true => Some(Reason::NotLeading),
            false => None,
        });
    };
    let Some(close) = closing_paren(pattern, 4) else {
        return Err(Some(Reason::Unterminated));
    };
    let inner = &pattern[4..close];
    if let Some(reason) = unsupported_inside(inner) {
        return Err(Some(reason));
    }
    Ok(Split {
        inner: inner.to_owned(),
        negated,
        body: pattern[close + 1..].to_owned(),
    })
}

/// Whether `pattern` STARTS with a lookbehind, and whether it is the negative
/// one.
fn opens_lookbehind(pattern: &str) -> Option<bool> {
    match () {
        _ if pattern.starts_with("(?<=") => Some(false),
        _ if pattern.starts_with("(?<!") => Some(true),
        _ => None,
    }
}

/// Whether a lookbehind appears anywhere, which is what separates "refused for
/// some other reason" from "refused for its position".
fn finds_lookbehind(pattern: &str) -> bool {
    pattern.contains("(?<=") || pattern.contains("(?<!")
}

/// The index of the `)` closing the group that opened before `from`.
///
/// Escapes and character classes are walked rather than skipped by search: a
/// `)` inside `[()]` closes nothing, and `\)` is a literal.
fn closing_paren(pattern: &str, from: usize) -> Option<usize> {
    let bytes = pattern.as_bytes();
    let mut depth = 1usize;
    let mut index = from;
    let mut in_class = false;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'[' if !in_class => in_class = true,
            b']' if in_class => in_class = false,
            b'(' if !in_class => depth += 1,
            b')' if !in_class => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// The first construct inside a lookbehind that the runtime check cannot
/// answer, if there is one.
///
/// A nested lookbehind is NOT one of them: it asks about text further back,
/// which is inside the prefix the check is given.
fn unsupported_inside(inner: &str) -> Option<Reason> {
    let bytes = inner.as_bytes();
    let mut index = 0usize;
    let mut in_class = false;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => {
                let next = bytes.get(index + 1).copied();
                match next {
                    Some(b'b') | Some(b'B') if !in_class => {
                        return Some(Reason::WordBoundary);
                    }
                    Some(b'1'..=b'9') if !in_class => return Some(Reason::Backreference),
                    Some(b'k') if !in_class => return Some(Reason::Backreference),
                    _ => {}
                }
                index += 1;
            }
            b'[' if !in_class => in_class = true,
            b']' if in_class => in_class = false,
            b'$' if !in_class => return Some(Reason::EndAnchor),
            b'(' if !in_class => {
                if let Some(reason) = group_opener(&inner[index..]) {
                    return Some(reason);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// What the group opening at the head of `rest` is, if it is one the check
/// cannot answer.
///
/// Only three openers are fine: `(?:`, and the two lookbehinds. Everything
/// else either captures or looks forwards.
fn group_opener(rest: &str) -> Option<Reason> {
    for allowed in ["(?:", "(?<=", "(?<!"] {
        if rest.starts_with(allowed) {
            return None;
        }
    }
    match rest.starts_with("(?=") || rest.starts_with("(?!") {
        true => Some(Reason::LooksAhead),
        false => Some(Reason::Captures),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(pattern: &str) -> (String, bool, String) {
        let split = split(pattern).ok().expect("splits");
        (split.inner, split.negated, split.body)
    }

    #[test]
    fn takes_the_leading_lookbehind_off() {
        assert_eq!(
            parts(r"(?<=\.\s*)[a-z]+"),
            (r"\.\s*".to_owned(), false, "[a-z]+".to_owned())
        );
        assert_eq!(
            parts(r"(?<!\.\s*)[a-z]+"),
            (r"\.\s*".to_owned(), true, "[a-z]+".to_owned())
        );
    }

    #[test]
    fn a_nested_group_does_not_end_the_lookbehind_early() {
        assert_eq!(
            parts(r"(?<=(?:ab)+)c"),
            ("(?:ab)+".to_owned(), false, "c".to_owned())
        );
    }

    #[test]
    fn a_paren_in_a_class_or_escaped_closes_nothing() {
        assert_eq!(
            parts(r"(?<=[()]+)x"),
            ("[()]+".to_owned(), false, "x".to_owned())
        );
        assert_eq!(
            parts(r"(?<=\)+)x"),
            (r"\)+".to_owned(), false, "x".to_owned())
        );
    }

    #[test]
    fn a_pattern_with_no_lookbehind_is_not_this_modules_business() {
        assert!(matches!(split(r"[a-z]+(?=x)"), Err(None)));
    }

    #[test]
    fn a_lookbehind_that_is_not_first_says_so() {
        assert!(matches!(
            split(r"x(?<=a+)b"),
            Err(Some(Reason::NotLeading))
        ));
    }

    #[test]
    fn each_unsupported_inside_is_named() {
        assert!(matches!(
            split(r"(?<=(a+))b"),
            Err(Some(Reason::Captures))
        ));
        assert!(matches!(
            split(r"(?<=(?<n>a+))b"),
            Err(Some(Reason::Captures))
        ));
        assert!(matches!(
            split(r"(?<=a(?=b)+)c"),
            Err(Some(Reason::LooksAhead))
        ));
        assert!(matches!(split(r"(?<=a$*)b"), Err(Some(Reason::EndAnchor))));
        assert!(matches!(
            split(r"(?<=\b+)b"),
            Err(Some(Reason::WordBoundary))
        ));
        assert!(matches!(
            split(r"(?<=\1+)b"),
            Err(Some(Reason::Backreference))
        ));
        assert!(matches!(split(r"(?<=a+"), Err(Some(Reason::Unterminated))));
    }

    #[test]
    fn a_dollar_or_a_boundary_inside_a_class_is_a_literal() {
        assert_eq!(
            parts(r"(?<=[$b]+)x"),
            ("[$b]+".to_owned(), false, "x".to_owned())
        );
    }
}
