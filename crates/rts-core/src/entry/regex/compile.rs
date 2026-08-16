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
pub(super) enum Engine {
    /// The linear-time one. What almost every pattern gets.
    Plain(regex::Regex),
    /// The backtracking one, for a pattern the first declined.
    Fancy(fancy_regex::Regex),
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
        // The three rewrites that are EXACT — see [`super::translate`] for why
        // the list stops at three.
        use super::translate::{empty_classes, unescape_solidus, wide_dot};
        let pattern = empty_classes(&unescape_solidus(pattern));
        let pattern = match flags.dot_all {
            true => pattern,
            // With `s` the builder below already says the whole set is allowed.
            false => wide_dot(&pattern),
        };
        match regex::RegexBuilder::new(&pattern)
            .case_insensitive(flags.ignore_case)
            .multi_line(flags.multiline)
            .dot_matches_new_line(flags.dot_all)
            .build()
        {
            Ok(compiled) => Some(Engine::Plain(compiled)),
            // Declined — which is usually a feature it does not have rather than
            // a pattern nobody can read, so the second engine is asked before
            // giving up.
            Err(_) => fancy_regex::Regex::new(&flags.inline(&pattern))
                .ok()
                .map(Engine::Fancy),
        }
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
        }
    }
}

/// What the letters after the closing slash mean.
///
/// A structure rather than the text, because three of them change how the
/// pattern is compiled and three change how a match is *driven* — and reading
/// the string again at every match to find out which is a table stated twice.
#[derive(Clone, Copy, Default)]
pub(super) struct Flags {
    /// `i`.
    pub(super) ignore_case: bool,
    /// `m` — `^` and `$` match at a line break.
    pub(super) multiline: bool,
    /// `s` — `.` matches a line break too.
    pub(super) dot_all: bool,
    /// `g` — the search resumes from `lastIndex` and advances it.
    pub(super) global: bool,
    /// `y` — the match must begin exactly at `lastIndex`.
    pub(super) sticky: bool,
    /// `d` — a match also says WHERE each group was, in `indices`.
    ///
    /// Nothing about compiling the pattern changes: both engines already answer
    /// the span of every group and the runtime was throwing all but the first
    /// away. So this is a flag about what a match ANSWERS, which is why it sits
    /// beside `global` and `sticky` rather than beside the three the builder
    /// reads.
    pub(super) has_indices: bool,
}

impl Flags {
    /// Reads the flag letters, refusing one the language does not have.
    ///
    /// `None` for an unknown letter rather than ignoring it: `/a/q` is a
    /// `SyntaxError` in JavaScript, and silently accepting it would make a typo
    /// into a regular expression that quietly means something else.
    ///
    /// `u` and `v` are **accepted and not acted on**. That is a stated
    /// divergence rather than an oversight: `u` changes what a `.` is when the
    /// subject has astral characters. Refusing them would refuse programs this
    /// engine otherwise runs correctly for every input they actually have.
    ///
    /// `d` was in that list and no longer is — see [`Flags::has_indices`].
    pub(super) fn parse(text: &str) -> Option<Flags> {
        let mut flags = Flags::default();
        let mut seen: Vec<char> = Vec::new();
        for letter in text.chars() {
            // A REPEATED letter is a `SyntaxError`, and so is `u` beside `v`.
            // Both were accepted, and both are the shape a hand-edited flag
            // string takes: `/a/gg` read as `/a/g`, and `/a/uv` as a pattern in
            // two mutually exclusive class grammars at once. Accepting either
            // turns a typo into a regular expression that quietly runs.
            if seen.contains(&letter) {
                return None;
            }
            if (letter == 'u' && seen.contains(&'v')) || (letter == 'v' && seen.contains(&'u')) {
                return None;
            }
            seen.push(letter);
            match letter {
                'i' => flags.ignore_case = true,
                'm' => flags.multiline = true,
                's' => flags.dot_all = true,
                'g' => flags.global = true,
                'y' => flags.sticky = true,
                'd' => flags.has_indices = true,
                'u' | 'v' => {}
                _ => return None,
            }
        }
        Some(flags)
    }

    /// The letters in the order `RegExp.prototype.flags` answers them.
    ///
    /// `re.flags` is BUILT by the specification's getter, one flag at a time in
    /// a fixed order — it is not the text the program wrote. So `/a/yusimgd`
    /// answers `"dgimsuy"`, and echoing the written order made every program
    /// that compares two patterns by their flags string disagree with itself
    /// over the same set. `u` and `v` are read from the letters because
    /// [`Flags`] has no field for them; see [`Flags::parse`].
    pub(super) fn canonical(self, letters: &str) -> String {
        let order: [(char, bool); 8] = [
            ('d', self.has_indices),
            ('g', self.global),
            ('i', self.ignore_case),
            ('m', self.multiline),
            ('s', self.dot_all),
            ('u', letters.contains('u')),
            ('v', letters.contains('v')),
            ('y', self.sticky),
        ];
        order
            .into_iter()
            .filter_map(|(letter, on)| on.then_some(letter))
            .collect()
    }

    /// Whether a match resumes from `lastIndex` rather than from the start.
    pub(super) fn tracks_last_index(self) -> bool {
        self.global || self.sticky
    }

    /// The pattern with the flags written into it, for the engine that has no
    /// builder.
    ///
    /// `fancy-regex` takes options as inline groups, so the same three facts are
    /// spelled as a prefix. Written from the same structure the builder is
    /// configured from, so the two cannot disagree about what `i` means.
    fn inline(self, pattern: &str) -> String {
        let mut prefix = String::new();
        if self.ignore_case {
            prefix.push('i');
        }
        if self.multiline {
            prefix.push('m');
        }
        if self.dot_all {
            prefix.push('s');
        }
        if prefix.is_empty() {
            pattern.to_string()
        } else {
            format!("(?{prefix}){pattern}")
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
    fn an_unknown_flag_letter_is_refused_rather_than_ignored() {
        // `/a/q` is a SyntaxError, and a typo that silently compiled would be a
        // regular expression quietly meaning something else.
        assert!(Flags::parse("q").is_none());
        assert!(Flags::parse("gimsy").is_some());
        // Repeated, and the two class grammars at once — both `SyntaxError`.
        assert!(Flags::parse("gg").is_none());
        assert!(Flags::parse("uv").is_none());
        assert!(Flags::parse("vu").is_none());
    }

    #[test]
    fn the_d_letter_is_read_rather_than_swallowed() {
        // It was in the same arm as `u` and `v` — accepted so a program is not
        // refused, and then forgotten — so `m.indices` was `undefined` for a
        // pattern that asked for it by name.
        assert!(Flags::parse("d").expect("a known letter").has_indices);
        assert!(!Flags::parse("g").expect("a known letter").has_indices);
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
