//! Turning a pattern and its flags into something that can match.
//!
//! # Why two engines and not one
//!
//! `regex` refuses lookaround and backreferences, and that refusal is the whole
//! reason it can promise linear time: both features make a match depend on where
//! the engine has already been, which is what a backtracking search is for.
//! JavaScript spells both, so refusing them here would refuse ordinary
//! JavaScript.
//!
//! So a pattern is offered to `regex` first and falls back to `fancy-regex` when
//! it is declined. A program that never writes a lookahead never pays for
//! backtracking, and one that does gets an answer rather than a refusal — which
//! is the trade the old engine made for the same reason, and the one place its
//! design is copied deliberately.
//!
//! # Why the fallback is a variant and not a trait object
//!
//! Two implementations, both known here, both selected once at compilation of
//! the literal. A `Box<dyn>` would add an indirection per match to express a
//! choice that has exactly two answers and never changes after construction.

/// A pattern that has been compiled, by whichever engine took it.
///
/// Cloning one is an `Arc` bump in both engines, which is what lets
/// [`super::compiled`] hand the same program to every object that spells the
/// pattern the same way.

/// A top-level alternation, one branch at a time, so a lookbehind may lead a
/// BRANCH rather than only the whole pattern.
mod branches;
/// The flag letters, as the structure the builder and the matcher both read.
mod flags;
/// The lookbehind the runtime checks itself, and why one is refused.
mod guard;

use branches::Arm;
pub(in crate::entry) use flags::Flags;
pub(in crate::entry::regex) use guard::refusal_detail;

#[derive(Clone)]
pub(in crate::entry) enum Engine {
    /// The linear-time one. What almost every pattern gets.
    Plain(regex::Regex),
    /// The backtracking one, for a pattern the first declined.
    Fancy(fancy_regex::Regex),
    /// A pattern whose LEADING lookbehind neither engine can compile, because
    /// its width is not fixed, with the lookbehind checked here instead.
    ///
    /// See [`super::lookbehind`] for why this is a split rather than one of
    /// `translate`'s rewrites, and for the three things it costs.
    Guarded {
        /// `(?:inner)\z` — asks whether the lookbehind's content ends exactly
        /// at a position, by being given the text before that position and
        /// nothing else.
        ///
        /// Always the backtracking engine: `\z` is the only part `regex` would
        /// take, and the content is whatever a lookbehind may hold.
        look: fancy_regex::Regex,
        /// `(?<!` rather than `(?<=`.
        negated: bool,
        /// The rest of the pattern, which is what actually matches and whose
        /// group numbering is therefore the whole pattern's — the lookbehind
        /// captures nothing, which `lookbehind::split` refuses to let it.
        body: Box<Engine>,
    },
    /// A pattern neither engine compiles whose TOP-LEVEL ALTERNATION is taken
    /// apart, so that a lookbehind leading a branch governs that branch alone.
    ///
    /// See [`branches`] for why this is not merely a widening of `Guarded`:
    /// `Guarded` applied a leading lookbehind to every branch of the body and
    /// answered a wrong result for `/(?<=\.\s*)[a-z]+|z/`.
    Alternation(Vec<Arm>),
}

/// Where a capture group matched, in **bytes**.
///
/// `None` for a group that took part in no alternative — `/(a)|(b)/` leaves one
/// of the two unset, and the language says that is `undefined` rather than the
/// empty string. Collapsing the two would make `/(a)|(b)/.exec("b")[1]` read as
/// `""`, which compares equal to nothing a program would test for.
pub(super) type Spans = Vec<Option<(usize, usize)>>;

impl Engine {
    /// Compiles a pattern, trying the fast engine and then the complete one.
    ///
    /// `None` is a pattern neither engine accepts. The language throws a
    /// `SyntaxError` there; see [`super::regex_new`] for why this answers rather
    /// than throws.
    pub(super) fn compile(pattern: &str, flags: Flags) -> Option<Engine> {
        Engine::of_translated(&Engine::translated(pattern, flags), flags)
    }

    /// The two engines, over a pattern [`translated`] has already rewritten.
    ///
    /// Separate from [`Self::compile`] because the lookbehind split below
    /// compiles the two halves of an already-translated pattern, and running
    /// the rewrites a second time over text they wrote is exactly what
    /// `translate`'s own ordering comments warn against.
    pub(super) fn of_translated(pattern: &str, flags: Flags) -> Option<Engine> {
        match regex::RegexBuilder::new(pattern)
            .case_insensitive(flags.ignore_case)
            .multi_line(flags.multiline)
            .dot_matches_new_line(flags.dot_all)
            .build()
        {
            Ok(compiled) => Some(Engine::Plain(compiled)),
            // Declined — which is usually a feature it does not have rather than
            // a pattern nobody can read, so the second engine is asked before
            // giving up.
            Err(_) => match fancy_regex::Regex::new(&flags.inline(pattern)) {
                Ok(compiled) => Some(Engine::Fancy(compiled)),
                // And the one thing the second engine does not have either: a
                // lookbehind whose width is not fixed. Asked LAST so that
                // nothing either engine already takes changes shape.
                Err(_) => guard::fallback(pattern, flags).ok(),
            },
        }
    }

    /// The pattern with every rewrite `translate` owns applied.
    ///
    /// Taken out of [`Self::compile`] so that [`refusal_detail`] words its
    /// message about the same text the engines were offered, rather than about
    /// what the program wrote — a `SyntaxError` naming a construct the
    /// rewrites had already removed would send its reader to the wrong place.
    fn translated(pattern: &str, flags: Flags) -> String {
        // The rewrites that are EXACT — see [`super::translate`] for why each
        // one is there and what is deliberately left alone.
        use super::translate::{
            astral_surrogate_classes, class_operators, empty_classes,
            forward_backreferences_as_empty, identity_escapes, legacy_octal_escapes,
            unescape_solidus, wide_dot,
        };
        // `class_operators` runs on what the PROGRAM wrote, before the rewrites
        // below inject Rust syntax of their own — `empty_classes` answers
        // `[^\s\S]` and `wide_dot` a bracketed set, and neither should be read
        // back as if a program had typed it.
        let pattern = identity_escapes(&unescape_solidus(pattern), flags.unicode);
        // Annex B's octal reading, and only there: the `u`/`v` grammar refuses
        // both readings rather than guessing, same as `identity_escapes`'s own
        // gate on `\p{...}` two lines up.
        let pattern = match flags.unicode {
            true => pattern,
            false => legacy_octal_escapes(&pattern),
        };
        // A forward reference is not something either engine can run at all —
        // see `forward_backreferences_as_empty`'s own doc for why — so this
        // runs under every flag combination, not only Annex B's.
        let pattern = forward_backreferences_as_empty(&pattern);
        // Before `class_operators`, which READS classes: this one rewrites the
        // inside of a class and what it writes is Rust syntax a program cannot
        // have typed, so it must not be read back as if one had.
        let pattern = astral_surrogate_classes(&pattern, flags.unicode);
        let pattern = class_operators(&pattern);
        let pattern = empty_classes(&pattern);
        let pattern = match flags.dot_all {
            true => pattern,
            // With `s` the builder below already says the whole set is allowed.
            false => wide_dot(&pattern),
        };
        pattern
    }

    /// Where the first match at or after `start` is, and where each group is.
    ///
    /// Byte offsets, because that is what both engines speak. The caller turns
    /// them into the UTF-16 positions JavaScript counts in — a conversion that
    /// belongs at the boundary rather than here, since neither engine has an
    /// opinion about it.
    /// Whether the pattern matches, without saying WHERE.
    ///
    /// `re.test(s)` answers a boolean and nothing else, and reaching that
    /// answer through [`Self::find_at`] costs two heap allocations it never
    /// reads: the capture set the engine fills in, and the vector of spans
    /// built from it. Measured: 280 ns for a three-character subject, of which
    /// only 85 more appear when the subject grows to 251 — so the cost was
    /// per CALL and not per character, which is what says it is the
    /// bookkeeping and not the scan.
    pub(super) fn matches_at(&self, haystack: &str, start: usize) -> bool {
        if start > haystack.len() {
            return false;
        }
        match self {
            Engine::Plain(compiled) => compiled.is_match_at(haystack, start),
            // No boolean form here that skips the capture set, so this is the
            // ordinary search with the answer discarded — still one allocation
            // fewer than building the spans, and stated rather than hidden.
            Engine::Fancy(compiled) => compiled
                .find_from_pos(haystack, start)
                .ok()
                .flatten()
                .is_some(),
            // No short answer here: which candidate the lookbehind accepts is
            // decided by WHERE the body matched, so the spans are part of
            // reaching the boolean rather than extra work beside it.
            Engine::Guarded { .. } => self.find_at(haystack, start).is_some(),
            // No leftmost and no ordering needed for a boolean: any branch
            // matching anywhere is the answer.
            Engine::Alternation(arms) => branches::matches_at(arms, haystack, start),
        }
    }

    /// The name of each capture group, by position, `None` for an unnamed one.
    ///
    /// Both engines expose this and neither was asked: `Spans` is indexed by
    /// position and carries no name at all, so a named group reached the
    /// runtime as an anonymous one and `m.groups` had nothing to be built from.
    /// Position zero is the whole match, which is never named — kept in the
    /// list so the index means the same thing here as it does in `Spans`.
    pub(super) fn names(&self) -> Vec<Option<String>> {
        match self {
            Engine::Plain(compiled) => compiled
                .capture_names()
                .map(|name| name.map(str::to_owned))
                .collect(),
            Engine::Fancy(compiled) => compiled
                .capture_names()
                .map(|name| name.map(str::to_owned))
                .collect(),
            // The body's, unchanged: the lookbehind holds no group at all —
            // `lookbehind::split` refuses one — so the numbering the body has
            // is the whole pattern's.
            Engine::Guarded { body, .. } => body.names(),
            // Spliced, because a group is numbered by where its `(` appears in
            // the WHOLE pattern and the branches were compiled apart.
            Engine::Alternation(arms) => branches::names(arms),
        }
    }

    /// Whether ANY group is named, without building the list of names.
    ///
    /// [`Self::names`] allocates a `Vec` and owns a `String` per named group,
    /// and `Regexp::named_groups` called it on every match of every pattern —
    /// including the overwhelming majority that name nothing, where the whole
    /// of that work produces an empty answer. This is the question those
    /// callers were really asking, and it is a walk over borrowed names with
    /// no allocation at all.
    pub(super) fn has_names(&self) -> bool {
        match self {
            Engine::Plain(compiled) => compiled.capture_names().any(|name| name.is_some()),
            Engine::Fancy(compiled) => compiled.capture_names().any(|name| name.is_some()),
            Engine::Guarded { body, .. } => body.has_names(),
            Engine::Alternation(arms) => branches::has_names(arms),
        }
    }

    pub(super) fn find_at(&self, haystack: &str, start: usize) -> Option<Spans> {
        if start > haystack.len() {
            return None;
        }
        match self {
            Engine::Plain(compiled) => {
                let found = compiled.captures_at(haystack, start)?;
                Some(
                    found
                        .iter()
                        .map(|group| group.map(|m| (m.start(), m.end())))
                        .collect(),
                )
            }
            Engine::Fancy(compiled) => {
                let found = compiled.captures_from_pos(haystack, start).ok()??;
                Some(
                    found
                        .iter()
                        .map(|group| group.map(|m| (m.start(), m.end())))
                        .collect(),
                )
            }
            // The search is `guard`'s, beside the check it drives: which
            // candidate is accepted is the same question as how a rejected one
            // is skipped, and splitting them puts one rule in two files.
            Engine::Guarded {
                look,
                negated,
                body,
            } => guard::find(look, *negated, body, haystack, start),
            // Likewise the branch search and the rule that picks between two
            // branches are one question, so they live together.
            Engine::Alternation(arms) => branches::find(arms, haystack, start),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn a_dot_excludes_the_four_line_terminators_and_not_only_the_newline() {
        let flags = Flags::parse("").expect("no flags");
        let engine = Engine::compile("a.b", flags).expect("compiles");
        assert!(engine.find_at("axb", 0).is_some());
        for terminator in ["\n", "\r", "\u{2028}", "\u{2029}"] {
            assert!(
                engine.find_at(&format!("a{terminator}b"), 0).is_none(),
                "`.` must not match a line terminator without `s`"
            );
        }
        // With `s` every one of them is allowed, and the translation is not
        // applied at all.
        let all = Engine::compile("a.b", Flags::parse("s").expect("s")).expect("compiles");
        assert!(all.find_at("a\rb", 0).is_some());
    }

    #[test]
    fn the_empty_class_matches_nothing_and_its_complement_matches_everything() {
        let never = Engine::compile("[]", Flags::default()).expect("`[]` is legal JavaScript");
        assert!(!never.matches_at("a", 0));
        assert!(!never.matches_at("\n", 0));
        let always = Engine::compile("[^]", Flags::default()).expect("`[^]` is legal JavaScript");
        assert!(always.matches_at("a", 0));
        assert!(always.matches_at("\n", 0));
    }

    #[test]
    fn a_lookahead_compiles_through_the_second_engine() {
        // The whole reason there are two: `regex` refuses this by construction,
        // and refusing it here would refuse ordinary JavaScript.
        let flags = Flags::parse("").expect("no flags");
        let engine = Engine::compile("foo(?=bar)", flags).expect("one of the two takes it");
        assert!(matches!(engine, Engine::Fancy(_)));
        assert!(engine.find_at("foobar", 0).is_some());
        assert!(engine.find_at("foobaz", 0).is_none());
    }

    #[test]
    fn an_escaped_slash_is_the_slash_a_literal_had_to_hide() {
        let flags = Flags::parse("").expect("no flags");
        let engine = Engine::compile("a\\/b", flags).expect("compiles once the escape is undone");
        assert!(engine.find_at("a/b", 0).is_some());
    }

    #[test]
    fn a_group_that_took_part_in_no_alternative_is_absent_not_empty() {
        let flags = Flags::parse("").expect("no flags");
        let engine = Engine::compile("(a)|(b)", flags).expect("compiles");
        let spans = engine.find_at("b", 0).expect("matches the second alternative");
        assert_eq!(spans[1], None, "`undefined`, which is not the empty string");
        assert_eq!(spans[2], Some((0, 1)));
    }

}
