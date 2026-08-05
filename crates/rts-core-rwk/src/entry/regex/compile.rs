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
        let pattern = unescape_solidus(pattern);
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
}

impl Flags {
    /// Reads the flag letters, refusing one the language does not have.
    ///
    /// `None` for an unknown letter rather than ignoring it: `/a/q` is a
    /// `SyntaxError` in JavaScript, and silently accepting it would make a typo
    /// into a regular expression that quietly means something else.
    ///
    /// `u`, `v` and `d` are **accepted and not acted on**. That is a stated
    /// divergence rather than an oversight: `u` changes what a `.` is when the
    /// subject has astral characters, and `d` adds an `indices` property to a
    /// match. Refusing them would refuse programs this engine otherwise runs
    /// correctly for every input they actually have.
    pub(super) fn parse(text: &str) -> Option<Flags> {
        let mut flags = Flags::default();
        for letter in text.chars() {
            match letter {
                'i' => flags.ignore_case = true,
                'm' => flags.multiline = true,
                's' => flags.dot_all = true,
                'g' => flags.global = true,
                'y' => flags.sticky = true,
                'u' | 'v' | 'd' => {}
                _ => return None,
            }
        }
        Some(flags)
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
fn unescape_solidus(pattern: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_flag_letter_is_refused_rather_than_ignored() {
        // `/a/q` is a SyntaxError, and a typo that silently compiled would be a
        // regular expression quietly meaning something else.
        assert!(Flags::parse("q").is_none());
        assert!(Flags::parse("gimsy").is_some());
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
