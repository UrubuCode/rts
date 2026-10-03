//! The lookbehind this runtime checks itself.
//!
//! `super::super::lookbehind` decides WHICH lookbehind that can be and words
//! every refusal; this is where the two halves are compiled and where the
//! question "does it end exactly here" is asked.

use super::super::lookbehind::{self, Reason};
use super::{Engine, Flags, Spans};

/// Whether `look`'s pattern ends exactly at `at`.
///
/// The engine is handed the text BEFORE `at` and a pattern anchored to the end
/// of it, so it is free to start the match anywhere in that prefix — which is
/// what lifts the width limit that made this module necessary. An error from
/// the engine (a backtrack limit) reads as "did not match", the same answer
/// `matches_at` gives it.
fn ends_at(look: &fancy_regex::Regex, haystack: &str, at: usize) -> bool {
    look.is_match(&haystack[..at]).unwrap_or(false)
}

/// The [`Engine::Guarded`] form of `pattern`, or why it cannot have one.
///
/// `Err(None)` is a pattern with no lookbehind in it: it was refused for some
/// other reason and this module has nothing to add.
pub(super) fn guarded(
    pattern: &str,
    flags: Flags,
) -> Result<Engine, Option<Reason>> {
    let split = lookbehind::split(pattern)?;
    // `\z` and not `$`: with `m` in the flags `$` is a LINE end, and the
    // prefix this is matched against ends wherever the candidate match begins
    // — so `$` would hold at every line break in the subject.
    let anchored = flags.inline(&format!(r"(?:{})\z", split.inner));
    let Ok(look) = fancy_regex::Regex::new(&anchored) else {
        return Err(None);
    };
    let Some(body) = Engine::of_translated(&split.body, flags) else {
        return Err(None);
    };
    Ok(Engine::Guarded {
        look,
        negated: split.negated,
        body: Box::new(body),
    })
}

/// What the `SyntaxError` for a refused pattern can say beyond the pattern
/// itself.
///
/// `None` when there is nothing to add, which is every refusal that is not
/// about a lookbehind. Asked from the throwing path only, which is why it pays
/// for the rewrites a second time rather than threading a reason back through
/// the cache in [`super::compiled`].
pub(in crate::entry::regex) fn refusal_detail(pattern: &str, flags: Flags) -> Option<String> {
    let translated = Engine::translated(pattern, flags);
    let reason = guarded(&translated, flags).err()??;
    Some(reason.message().to_owned())
}

/// Where the first match of a guarded pattern at or after `start` is.
///
/// The body's leftmost match at or after `start` is the leftmost place the
/// WHOLE pattern could begin — the lookbehind is zero-width, so the two starts
/// are the same position. A candidate the lookbehind rejects is therefore
/// skipped by ONE CHARACTER rather than by the length of the body's match:
/// `/(?<!\.\s*)[a-z]+/` over `"a. foo"` answers `"oo"` for its second match in
/// both Node and Bun, and that is the candidate one character into a rejected
/// one.
pub(super) fn find(
    look: &fancy_regex::Regex,
    negated: bool,
    body: &Engine,
    haystack: &str,
    start: usize,
) -> Option<Spans> {
    let mut from = start;
    loop {
        let spans = body.find_at(haystack, from)?;
        let at = spans[0]?.0;
        if ends_at(look, haystack, at) != negated {
            return Some(spans);
        }
        from = at + haystack[at..].chars().next()?.len_utf8();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every match of `pattern` over `subject`, driven the way `g` drives one.
    fn all(pattern: &str, letters: &str, subject: &str) -> Vec<(usize, usize)> {
        let flags = Flags::parse(letters).expect("flags");
        let engine = Engine::compile(pattern, flags).expect("compiles");
        let mut found = Vec::new();
        let mut from = 0usize;
        while let Some(spans) = engine.find_at(subject, from) {
            let (start, end) = spans[0].expect("group zero is always set");
            found.push((start, end));
            // What `lastIndex` does, including the empty-match nudge.
            from = match end > start {
                true => end,
                false => end + 1,
            };
        }
        found
    }

    fn text(subject: &str, spans: &[(usize, usize)]) -> Vec<String> {
        spans
            .iter()
            .map(|(start, end)| subject[*start..*end].to_owned())
            .collect()
    }

    /// The five forms issue #2891 measured, each against what Node 22 and Bun
    /// 1.4 both answered on 2026-10-03 — they agreed on every one.
    #[test]
    fn a_lookbehind_with_a_quantifier_answers_what_the_other_engines_answer() {
        let subject = "a. foo b.x c.  bar";
        assert_eq!(
            text(subject, &all(r"(?<=\.\s*)[a-z]+", "g", subject)),
            ["foo", "x", "bar"]
        );
        assert_eq!(text("cab", &all(r"(?<=a+)b", "", "cab")), ["b"]);
        assert!(all(r"(?<=a+)b", "", "b").is_empty());
        assert_eq!(text("xxy", &all(r"(?<=x{1,3})y", "", "xxy")), ["y"]);
        assert!(all(r"(?<=x{1,3})y", "", "y").is_empty());
        // The negative one, and the case that decides the SKIP: the candidate
        // at index 3 is rejected and the one at index 4 is not, so the answer
        // is `"oo"` and not nothing.
        assert_eq!(
            text("a. foo", &all(r"(?<!\.\s*)[a-z]+", "g", "a. foo")),
            ["a", "oo"]
        );
    }

    /// The three forms that already worked, asserted as still working: the
    /// lookbehind split is asked LAST, so nothing either engine takes changes
    /// shape.
    #[test]
    fn a_fixed_lookbehind_and_the_lookahead_still_go_to_the_two_engines() {
        let subject = "a.foo b.x c.bar";
        assert_eq!(
            text(subject, &all(r"(?<=\.)[a-z]+", "g", subject)),
            ["foo", "x", "bar"]
        );
        assert_eq!(text("abd cd xd", &all(r"(?<=ab|c)d", "g", "abd cd xd")), ["d", "d"]);
        assert!(Engine::compile(r"(?<ano>\d{4})", Flags::default()).is_some());
    }

    /// Zero repetitions at the very start of the subject is a legal match, and
    /// it is where an off-by-one in the prefix slice would live.
    #[test]
    fn a_lookbehind_that_can_match_nothing_holds_at_position_zero() {
        assert_eq!(text("abc", &all(r"(?<=x*)a", "", "abc")), ["a"]);
        assert_eq!(text("  a a", &all(r"(?<=\s*)a", "g", "  a a")), ["a", "a"]);
        assert_eq!(text("aab", &all(r"(?<=^a*)b", "", "aab")), ["b"]);
    }

    /// `$` in the BODY is the subject's end, and an empty match is still a
    /// match — `/(?<=a+)$/.exec("aaa")` answers `""` at 3 in both engines.
    #[test]
    fn the_body_may_be_an_anchor_alone() {
        assert_eq!(all(r"(?<=a+)$", "", "aaa"), [(3, 3)]);
    }

    /// A resumed search looks BEFORE where it resumes, which is the whole
    /// point of a lookbehind and the place a prefix built from `&subject[from..]`
    /// would answer differently.
    #[test]
    fn a_resumed_search_still_sees_the_text_before_it() {
        let subject = "a. foo b.x c.  bar";
        let flags = Flags::parse("g").expect("g");
        let engine = Engine::compile(r"(?<=\.\s*)[a-z]+", flags).expect("compiles");
        // Node and Bun: `lastIndex = 5` answers `"x"` at 9 with `lastIndex` 10.
        let spans = engine.find_at(subject, 5).expect("matches from five");
        assert_eq!(spans[0], Some((9, 10)));
        // And the `.` the lookbehind reads is at 8, which is BEFORE the
        // resumption point — the one at index 1 is not.
        assert_eq!(&subject[8..10], ".x");
    }

    /// A group in the BODY keeps the numbering the whole pattern has, because
    /// the lookbehind is not allowed to hold one.
    #[test]
    fn a_group_after_the_lookbehind_is_group_one() {
        let flags = Flags::parse("g").expect("g");
        let engine = Engine::compile(r"(?<=\.\s*)([a-z]+)", flags).expect("compiles");
        let spans = engine.find_at("a.  foo", 0).expect("matches");
        assert_eq!(spans[0], Some((4, 7)));
        assert_eq!(spans[1], Some((4, 7)));
        let named = Engine::compile(r"(?<=\.\s*)(?<word>[a-z]+)", flags).expect("compiles");
        assert!(named.has_names());
        assert_eq!(named.names()[1].as_deref(), Some("word"));
    }

    /// What stays refused, and that the message says WHICH form and why.
    ///
    /// `invalid regular expression: /<the pattern>/` was the whole message, and
    /// it is the one that helps nobody — the pattern is the part the reader
    /// already has.
    #[test]
    fn an_unreachable_form_is_refused_by_name() {
        for (pattern, says) in [
            (r"x(?<=a+)b", "first thing in the pattern"),
            (r"(?:(?<=a+)b)", "first thing in the pattern"),
            (r"(?<=(a+))b", "capture group inside a lookbehind"),
            (r"(?<=a+(?=b))c", "lookahead inside a lookbehind"),
        ] {
            assert!(
                Engine::compile(pattern, Flags::default()).is_none(),
                "{pattern} must be refused rather than answered approximately"
            );
            let detail = refusal_detail(pattern, Flags::default())
                .unwrap_or_else(|| panic!("{pattern} must say why"));
            assert!(detail.contains(says), "{pattern} said {detail:?}");
        }
        // And a refusal that is NOT about a lookbehind keeps the old message.
        assert_eq!(refusal_detail(r"(?<=a)(?!", Flags::default()), None);
    }
}
