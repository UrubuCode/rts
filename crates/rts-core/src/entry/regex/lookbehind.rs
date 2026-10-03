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
//! **First thing in the pattern, or first thing in a top-level BRANCH** — which
//! is the widening [`branches`] exists for and the reason it lives in this
//! module rather than beside the engines. Nothing else about the check changes:
//! each branch is still a pattern whose lookbehind is leading.
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
    /// A numbered backreference in a branch after the first of a top-level
    /// alternation that has to be split — see
    /// [`holds_numbered_backreference`] for why a split would answer a
    /// different regular expression rather than refuse.
    SplitBackreference,
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
            Reason::SplitBackreference => {
                "a numbered backreference after the first branch of an alternation holding a lookbehind of unbounded width is not supported"
            }
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
pub(super) fn finds_lookbehind(pattern: &str) -> bool {
    pattern.contains("(?<=") || pattern.contains("(?<!")
}

/// The top-level alternatives of `pattern`, or `None` when it has only one.
///
/// # Why this is here, and what it is for
///
/// `(?<=\.\s*)x|z` is an alternation whose FIRST branch carries the
/// lookbehind, and the split above answered it by taking the lookbehind off the
/// front and matching `x|z` under the check — which applies the lookbehind to
/// `z` as well. Node and Bun answer `["z"]` for `/(?<=\.\s*)[a-z]+|z/` over
/// `"z"`; that reading answered `null`, silently. A lookbehind inside the
/// SECOND branch was refused outright.
///
/// Both are one shape: the lookbehind is leading *within its branch*. So the
/// branches are compiled separately and recombined by the rule JavaScript
/// actually has — the leftmost position any branch matches at, and on a tie the
/// EARLIEST branch, because an alternation is ordered rather than greedy.
/// See [`super::compile`]'s alternation module for that recombination.
pub(super) fn branches(pattern: &str) -> Option<Vec<&str>> {
    let bars: Vec<usize> = Scan::over(pattern, 0)
        .filter(|step| step.byte == b'|' && !step.in_class && step.depth == 0)
        .map(|step| step.index)
        .collect();
    if bars.is_empty() {
        return None;
    }
    let mut parts = Vec::with_capacity(bars.len() + 1);
    let mut from = 0usize;
    for bar in bars {
        parts.push(&pattern[from..bar]);
        from = bar + 1;
    }
    parts.push(&pattern[from..]);
    Some(parts)
}

/// Whether a NUMBERED backreference appears in `pattern` outside a character
/// class.
///
/// # Why the number is what matters, and only after the first branch
///
/// Splitting a top-level alternation renumbers every group after the first
/// branch, so `\1` in a LATER branch comes to name a different group:
/// `/(a)|(?<=x*)(b)\1/` over `"bb"` answers `"b"` in node 22 and bun 1.4 — a
/// backreference to a group that took part in nothing matches the empty string
/// — and a split branch of `(b)\1` would answer `"bb"`. A wrong answer rather
/// than a refusal, so that one is declined.
///
/// The FIRST branch keeps its numbering exactly (its groups still start at 1),
/// which is why this is asked of the later branches only. That is not a
/// nicety: `/(?<=x*)(b)\1|(a)/` is a pattern the leading-lookbehind split
/// already answers correctly today, and refusing it to buy a simpler rule
/// would be a capability lost to a tidier implementation.
///
/// A NAMED backreference is not counted at all. A name cannot be renumbered,
/// and one naming a group in another branch leaves that branch with no such
/// name — which either refuses loudly or matches the empty string, and the
/// empty string is what the language answers for it anyway.
pub(super) fn holds_numbered_backreference(pattern: &str) -> bool {
    Scan::over(pattern, 0)
        .any(|step| step.byte == b'\\' && !step.in_class && matches!(step.escaped, Some(b'1'..=b'9')))
}

/// The index of the `)` closing the group that opened before `from`.
fn closing_paren(pattern: &str, from: usize) -> Option<usize> {
    Scan::over(pattern, from)
        .find(|step| step.byte == b')' && !step.in_class && step.depth == 0)
        .map(|step| step.index)
}

/// The first construct inside a lookbehind that the runtime check cannot
/// answer, if there is one.
///
/// A nested lookbehind is NOT one of them: it asks about text further back,
/// which is inside the prefix the check is given.
fn unsupported_inside(inner: &str) -> Option<Reason> {
    Scan::over(inner, 0).find_map(|step| match step.in_class {
        true => None,
        false => match step.byte {
            b'\\' => match step.escaped {
                Some(b'b') | Some(b'B') => Some(Reason::WordBoundary),
                Some(b'1'..=b'9') | Some(b'k') => Some(Reason::Backreference),
                _ => None,
            },
            b'$' => Some(Reason::EndAnchor),
            b'(' => group_opener(&inner[step.index..]),
            _ => None,
        },
    })
}

/// One byte of a pattern, with what a reader of regular-expression syntax has
/// to know about it.
struct Step {
    /// Where it is, so a caller can slice from it.
    index: usize,
    byte: u8,
    /// The byte a backslash escapes, which the scan does not report on its own.
    escaped: Option<u8>,
    in_class: bool,
    /// For `(`, the depth the group opens AT; for `)`, the depth it closes
    /// FROM. Asymmetric deliberately: it makes `depth == 0` on a `)` mean "this
    /// one closes a group that opened before the scan started", which is the
    /// question [`closing_paren`] asks and the only reading of the two that is
    /// unambiguous for both parentheses of the same group.
    depth: usize,
}

/// Walks a pattern's bytes, tracking character classes, escapes and group
/// depth.
///
/// # Why one traversal rather than one per question
///
/// Four questions are asked of a pattern's bytes here — where a group closes,
/// where a top-level `|` is, which construct a lookbehind may not hold, and
/// whether a backreference appears — and every one of them has to know that a
/// `)` inside `[()]` closes nothing and that `\)` is a literal. Four copies of
/// that rule are four chances for one to be written differently, and the
/// failure that produces is a pattern split in the wrong place: a regular
/// expression that compiles and means something else.
struct Scan<'a> {
    bytes: &'a [u8],
    index: usize,
    in_class: bool,
    depth: usize,
}

impl<'a> Scan<'a> {
    fn over(pattern: &'a str, from: usize) -> Scan<'a> {
        Scan {
            bytes: pattern.as_bytes(),
            index: from,
            in_class: false,
            depth: 0,
        }
    }
}

impl Iterator for Scan<'_> {
    type Item = Step;

    fn next(&mut self) -> Option<Step> {
        let index = self.index;
        let byte = *self.bytes.get(index)?;
        let mut escaped = None;
        let in_class = self.in_class;
        let depth = self.depth;
        match byte {
            b'\\' => {
                escaped = self.bytes.get(index + 1).copied();
                self.index += 1;
            }
            b'[' if !in_class => self.in_class = true,
            b']' if in_class => self.in_class = false,
            b'(' if !in_class => self.depth += 1,
            // Saturating because a scan started INSIDE a group reaches that
            // group's own `)` with nothing left to subtract, and reporting
            // zero there is what tells `closing_paren` it has arrived.
            b')' if !in_class => self.depth = depth.saturating_sub(1),
            _ => {}
        }
        self.index += 1;
        Some(Step {
            index,
            byte,
            escaped,
            in_class,
            depth,
        })
    }
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
    fn only_a_top_level_bar_divides_the_branches() {
        assert_eq!(branches("a"), None);
        assert_eq!(branches("a|b|c"), Some(vec!["a", "b", "c"]));
        // Inside a group, inside a class, and escaped: none of the three is a
        // branch boundary, and splitting at one would compile a pattern that
        // means something else.
        assert_eq!(branches("(?:a|b)"), None);
        assert_eq!(branches("[a|b]"), None);
        assert_eq!(branches(r"a\|b"), None);
        assert_eq!(branches("(?:a|b)|c"), Some(vec!["(?:a|b)", "c"]));
        // An empty branch is legal JavaScript and matches the empty string.
        assert_eq!(branches("a|"), Some(vec!["a", ""]));
        assert_eq!(branches("|a"), Some(vec!["", "a"]));
    }

    #[test]
    fn a_numbered_backreference_is_found_only_where_it_is_one() {
        assert!(holds_numbered_backreference(r"(a)\1"));
        // A NAME cannot be renumbered, so it is not one of these.
        assert!(!holds_numbered_backreference(r"(?<n>a)\k<n>"));
        // `\0` is NUL, not a group; a digit inside a class is a member; and
        // `\\1` is a literal backslash followed by a one.
        assert!(!holds_numbered_backreference(r"a\0b"));
        assert!(!holds_numbered_backreference(r"[\1]"));
        assert!(!holds_numbered_backreference(r"a\\1"));
    }

    #[test]
    fn a_dollar_or_a_boundary_inside_a_class_is_a_literal() {
        assert_eq!(
            parts(r"(?<=[$b]+)x"),
            ("[$b]+".to_owned(), false, "x".to_owned())
        );
    }
}
