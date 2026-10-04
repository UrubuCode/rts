//! A top-level alternation, compiled one branch at a time.
//!
//! # Why a pattern is ever taken apart this way
//!
//! [`super::guard`] lifts the width limit on a lookbehind that is the FIRST
//! thing in a pattern. An alternation moves the goalposts twice over:
//!
//! - `/(?<=\.\s*)[a-z]+|z/` has its lookbehind leading, so the split took it
//!   and matched `[a-z]+|z` under the check — applying the lookbehind to `z`
//!   too. Node and Bun answer `["z"]` over `"z"`; that reading answered `null`.
//!   A **wrong answer**, not a refusal, and the shape the honesty floor calls
//!   the worst kind.
//! - `/z|(?<=\.\s*)[a-z]+/` has it leading inside the second branch, and was
//!   refused outright.
//!
//! Both are the same fact: in JavaScript a lookbehind at the head of a branch
//! governs that branch and nothing else. So the branches are compiled
//! separately — each one a pattern in its own right, which may be either engine
//! or a guarded lookbehind of its own — and recombined.
//!
//! # The recombination is the specification's, not a heuristic
//!
//! An alternation is **ordered**, not greedy: the engine tries each position
//! left to right and, at a position, each alternative in written order, taking
//! the first that matches. Over the whole subject that is exactly: the leftmost
//! position any branch matches at, and among the branches that match there, the
//! earliest one. [`find`] is those two sentences.
//!
//! What makes it exact rather than nearly right is that it is asked only of a
//! pattern **both engines already refused**, and that each branch is still
//! compiled as one unit — so greediness, laziness and the ordering of a nested
//! alternation stay the engine's business.
//!
//! # Why `lastIndex` needed nothing
//!
//! This was the expensive-looking part and it turned out to be free. The
//! obvious reading of "N patterns" is N positions to keep in step with the one
//! `lastIndex` a program can see. There are none: [`find`] takes the single
//! `start` it is given, asks every branch about that same position, and holds
//! nothing between calls. `lastIndex` stays what it was — an ordinary property
//! of the object, written by the program and read by
//! [`super::super::Regexp::find_at`] — and `exec` in a loop, `matchAll`,
//! `replace`, `split` and a written `lastIndex` all drive this variant through
//! the identical path they drive a `Plain` one through.
//!
//! # What is refused, and why that one is not negotiable
//!
//! A NUMBERED backreference in a branch after the first. Splitting renumbers
//! every group after the first branch, so `\1` would come to name a different
//! group — `/(a)|(?<=x*)(b)\1/` over `"bb"` answers `"b"` in node 22 and bun
//! 1.4, and a branch of `(b)\1` answers `"bb"`. The first branch keeps its
//! numbering and so keeps its backreferences; see
//! [`super::super::lookbehind::holds_numbered_backreference`] for why a name is
//! never one of these.

use super::super::lookbehind::{self, Reason};
use super::{Engine, Flags, Spans};

/// One branch, with where its capture groups sit in the WHOLE pattern's
/// numbering.
///
/// The offset is what makes `match[1]` mean the same thing it means in every
/// other engine: groups are numbered across the whole pattern by the order
/// their `(` appears, so the second branch's first group is not group 1 unless
/// the first branch has none.
#[derive(Clone)]
pub(in crate::entry) struct Arm {
    engine: Engine,
    /// The number the branch's own group 1 has in the whole pattern.
    first_group: usize,
    /// How many groups the branch has, so [`Arm::widen`] needs no second walk.
    groups: usize,
}

impl Arm {
    /// The branch's spans, placed in a vector the size the whole pattern's
    /// group list is.
    ///
    /// Every group of every OTHER branch is `None` — which is what the
    /// language says a group that took part in no alternative is, and is
    /// already how [`Spans`] spells `undefined`.
    fn widen(&self, spans: Spans, total: usize) -> Spans {
        let mut whole = vec![None; total + 1];
        whole[0] = spans[0];
        for (offset, span) in spans.iter().skip(1).enumerate() {
            whole[self.first_group + offset] = *span;
        }
        whole
    }
}

/// How many groups the whole alternation has.
fn total(arms: &[Arm]) -> usize {
    arms.last()
        .map(|arm| arm.first_group + arm.groups - 1)
        .unwrap_or(0)
}

/// The [`Engine::Alternation`] form of a pattern whose top-level branches
/// `parts` are, or why it cannot have one.
///
/// `Err(None)` is "this module has nothing to add to the refusal" — a pattern
/// where no branch mentions a lookbehind at all was refused for some other
/// reason, and inventing a sentence about alternation would send its reader to
/// the wrong place.
pub(super) fn compiled(parts: &[&str], flags: Flags) -> Result<Engine, Option<Reason>> {
    let about_a_lookbehind = parts
        .iter()
        .any(|part| lookbehind::finds_lookbehind(part));
    let detail = |reason: Reason| match about_a_lookbehind {
        true => Err(Some(reason)),
        false => Err(None),
    };
    // The FIRST branch keeps its numbering exactly, so it is skipped: a
    // leading lookbehind beside a backreference in its own branch is a pattern
    // this runtime already answers correctly, and refusing it to buy a simpler
    // rule would lose a capability to a tidier implementation.
    if parts
        .iter()
        .skip(1)
        .any(|part| lookbehind::holds_numbered_backreference(part))
    {
        return detail(Reason::SplitBackreference);
    }
    let mut arms: Vec<Arm> = Vec::with_capacity(parts.len());
    let mut next_group = 1usize;
    for part in parts {
        let Some(engine) = Engine::of_translated(part, flags) else {
            // The branch's own reason, asked the way `refusal_detail` asks
            // one: paid for only on the path that is about to throw.
            return Err(super::guard::fallback(part, flags).err().flatten());
        };
        let groups = engine.names().len() - 1;
        arms.push(Arm {
            engine,
            first_group: next_group,
            groups,
        });
        next_group += groups;
    }
    Ok(Engine::Alternation(arms))
}

/// Where the first match of the alternation at or after `start` is.
///
/// The leftmost position any branch matches at, and on a tie the earliest
/// branch — see this module's second page for why those two sentences are the
/// whole rule.
pub(super) fn find(arms: &[Arm], haystack: &str, start: usize) -> Option<Spans> {
    let mut best: Option<(usize, &Arm, Spans)> = None;
    for arm in arms {
        let Some(spans) = arm.engine.find_at(haystack, start) else {
            continue;
        };
        let at = spans[0]?.0;
        // `<=` and not `<`: an earlier branch already holding this position
        // KEEPS it, which is the ordering half of the rule.
        if best.as_ref().is_some_and(|(seen, _, _)| *seen <= at) {
            continue;
        }
        best = Some((at, arm, spans));
    }
    let (_, arm, spans) = best?;
    Some(arm.widen(spans, total(arms)))
}

/// Whether any branch matches, which needs no leftmost and no ordering.
pub(super) fn matches_at(arms: &[Arm], haystack: &str, start: usize) -> bool {
    arms.iter()
        .any(|arm| arm.engine.matches_at(haystack, start))
}

/// The name of each group of the whole pattern, by its whole-pattern position.
pub(super) fn names(arms: &[Arm]) -> Vec<Option<String>> {
    let mut all = vec![None; total(arms) + 1];
    for arm in arms {
        for (offset, name) in arm.engine.names().into_iter().skip(1).enumerate() {
            all[arm.first_group + offset] = name;
        }
    }
    all
}

/// Whether any group anywhere is named.
pub(super) fn has_names(arms: &[Arm]) -> bool {
    arms.iter().any(|arm| arm.engine.has_names())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every match of `pattern` over `subject`, driven the way `g` drives one.
    fn all(pattern: &str, letters: &str, subject: &str) -> Vec<String> {
        let flags = Flags::parse(letters).expect("flags");
        let engine = Engine::compile(pattern, flags).expect("compiles");
        let mut found = Vec::new();
        let mut from = 0usize;
        while let Some(spans) = engine.find_at(subject, from) {
            let (start, end) = spans[0].expect("group zero is always set");
            found.push(subject[start..end].to_owned());
            from = match end > start {
                true => end,
                false => end + 1,
            };
        }
        found
    }

    /// The two forms the split exists for, against what node 22 and bun 1.4
    /// both answered on 2026-10-03 — they agreed on every one.
    ///
    /// The first was answering `["foo","x"]`, dropping the `z`: the leading
    /// lookbehind was being applied to the OTHER branch as well.
    #[test]
    fn a_lookbehind_governs_its_own_branch_and_no_other() {
        assert_eq!(
            all(r"(?<=\.\s*)[a-z]+|z", "g", "a. foo z b.x"),
            ["foo", "z", "x"]
        );
        assert_eq!(all(r"(?<=\.\s*)[a-z]+|z", "g", "z"), ["z"]);
        assert_eq!(all(r"z|(?<=\.\s*)[a-z]+", "g", "a. foo z"), ["foo", "z"]);
    }

    /// The pattern issue #2891 was raised for: `kire`'s identifier scanner,
    /// three top-level branches with the variable lookbehind in the second.
    #[test]
    fn the_identifier_scanner_of_the_issue_answers_what_the_others_answer() {
        let pattern = concat!(
            r"(?:['",
            "\"",
            r"`].*?['",
            "\"",
            r"`])|(?<=\.\s*)[a-zA-Z_$][a-zA-Z0-9_$]*",
            r"|(?<![a-zA-Z0-9_$])([a-zA-Z_$][a-zA-Z0-9_$]*)(?![a-zA-Z0-9_$])"
        );
        assert_eq!(
            all(pattern, "g", "u . nome + \"s\" + a.b"),
            ["u", "nome", "\"s\"", "a", "b"]
        );
    }

    /// A tie of POSITION is decided by the written order of the branches,
    /// because an alternation is ordered rather than greedy: both branches can
    /// match at 0 and `/(?<=x*)ab|a/` answers `"ab"` while the same two
    /// branches the other way round answer `"a"`.
    #[test]
    fn a_tie_goes_to_the_earlier_branch_and_not_to_the_longer_match() {
        assert_eq!(all(r"(?<=x*)ab|a", "", "ab"), ["ab"]);
        assert_eq!(all(r"a|(?<=x*)ab", "", "ab"), ["a"]);
    }

    /// Group numbering is the WHOLE pattern's, and a group in another branch is
    /// absent rather than empty — which is what `match[1]` being `undefined`
    /// means, and what the issue's own pattern reads.
    #[test]
    fn groups_are_numbered_across_the_branches() {
        let flags = Flags::default();
        let engine =
            Engine::compile(r"(?<=x*)(a)(b)|(c)", flags).expect("compiles");
        let spans = engine.find_at("ab", 0).expect("the first branch matches");
        assert_eq!(spans.len(), 4, "one for the match and three groups");
        assert_eq!(spans[1], Some((0, 1)));
        assert_eq!(spans[2], Some((1, 2)));
        assert_eq!(spans[3], None, "`undefined`, which is not the empty string");
        let other = engine.find_at("c", 0).expect("the second branch matches");
        assert_eq!(other[1], None);
        assert_eq!(other[2], None);
        assert_eq!(other[3], Some((0, 1)));
    }

    /// And the names follow the same numbering, so `groups` is built from the
    /// right positions.
    #[test]
    fn a_named_group_in_a_later_branch_keeps_its_position() {
        let engine = Engine::compile(r"(?<=x*)(a)|(?<tail>c)", Flags::default())
            .expect("compiles");
        assert!(engine.has_names());
        let names = engine.names();
        assert_eq!(names.len(), 3);
        assert_eq!(names[1], None);
        assert_eq!(names[2].as_deref(), Some("tail"));
    }

    /// A backreference in a LATER branch is refused rather than renumbered —
    /// the one case where splitting would answer a different regular
    /// expression — and one in the FIRST branch keeps working, because its
    /// numbering is untouched.
    #[test]
    fn a_backreference_after_the_first_branch_refuses_the_split_by_name() {
        let flags = Flags::default();
        assert!(Engine::compile(r"(a)|(?<=x*)(b)\1", flags).is_none());
        let detail = super::super::refusal_detail(r"(a)|(?<=x*)(b)\1", flags)
            .expect("says why");
        assert!(
            detail.contains("numbered backreference after the first branch"),
            "{detail}"
        );
        // node 22 and bun 1.4 both answer `["bb","b",null]` for this one, and
        // so did the leading-lookbehind split before this change. Refusing it
        // would be a capability lost rather than a hazard avoided.
        assert_eq!(all(r"(?<=x*)(b)\1|(a)", "", "bb"), ["bb"]);
        assert!(all(r"(?<=x*)(b)\1|(a)", "", "b").is_empty());
    }

    /// A pattern neither engine takes and no branch has a lookbehind in keeps
    /// the message it had: this module has nothing to add to it.
    #[test]
    fn a_refusal_that_is_not_about_a_lookbehind_says_nothing_new() {
        assert_eq!(
            super::super::refusal_detail(r"(?!|(?!", Flags::default()),
            None
        );
    }

    /// An empty branch is legal and matches the empty string, and the tie rule
    /// still puts the written order first.
    #[test]
    fn an_empty_branch_matches_the_empty_string() {
        assert_eq!(all(r"(?<=x*)a|", "g", "ba"), ["", "a", ""]);
    }
}
