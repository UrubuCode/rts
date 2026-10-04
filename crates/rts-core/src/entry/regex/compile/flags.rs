//! The flag letters, read once into a structure.

/// What the letters after the closing slash mean.
///
/// A structure rather than the text, because three of them change how the
/// pattern is compiled and three change how a match is *driven* — and reading
/// the string again at every match to find out which is a table stated twice.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(in crate::entry) struct Flags {
    /// `i`.
    pub(in crate::entry::regex) ignore_case: bool,
    /// `m` — `^` and `$` match at a line break.
    pub(in crate::entry::regex) multiline: bool,
    /// `s` — `.` matches a line break too.
    pub(in crate::entry::regex) dot_all: bool,
    /// `g` — the search resumes from `lastIndex` and advances it.
    pub(in crate::entry::regex) global: bool,
    /// `y` — the match must begin exactly at `lastIndex`.
    pub(in crate::entry::regex) sticky: bool,
    /// `d` — a match also says WHERE each group was, in `indices`.
    ///
    /// Nothing about compiling the pattern changes: both engines already answer
    /// the span of every group and the runtime was throwing all but the first
    /// away. So this is a flag about what a match ANSWERS, which is why it sits
    /// beside `global` and `sticky` rather than beside the three the builder
    /// reads.
    pub(in crate::entry::regex) has_indices: bool,
    /// The Unicode-mode grammar.
    ///
    /// Kept, and read in exactly one place: `translate::identity_escapes`
    /// asks it because a property escape is one WITH the flag and two
    /// ordinary letters without it, while Rust read the property either way.
    pub(in crate::entry::regex) unicode: bool,
}

impl Flags {
    /// Reads the flag letters, refusing one the language does not have.
    ///
    /// `None` for an unknown letter rather than ignoring it: `/a/q` is a
    /// `SyntaxError` in JavaScript, and silently accepting it would make a typo
    /// into a regular expression that quietly means something else.
    ///
    /// The two Unicode flags are **acted on in one place only**, which is
    /// [`Flags::unicode`]. The rest is a stated divergence: the flag also
    /// subject has astral characters. Refusing them would refuse programs this
    /// engine otherwise runs correctly for every input they actually have.
    ///
    /// `d` was in that list and no longer is — see [`Flags::has_indices`].
    pub(in crate::entry::regex) fn parse(text: &str) -> Option<Flags> {
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
                'u' | 'v' => flags.unicode = true,
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
    pub(in crate::entry::regex) fn canonical(self, letters: &str) -> String {
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
    pub(in crate::entry::regex) fn tracks_last_index(self) -> bool {
        self.global || self.sticky
    }

    /// The pattern with the flags written into it, for the engine that has no
    /// builder.
    ///
    /// `fancy-regex` takes options as inline groups, so the same three facts are
    /// spelled as a prefix. Written from the same structure the builder is
    /// configured from, so the two cannot disagree about what `i` means.
    pub(in crate::entry::regex) fn inline(self, pattern: &str) -> String {
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
}
