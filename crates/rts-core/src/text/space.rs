//! Which code units the language calls white space.
//!
//! # Why this is a question about a UNIT and not about a `char`
//!
//! Because the strings it is asked about may not be valid Unicode. `" \u{D800} "`
//! is a legal JavaScript string, and `trim` has to answer for it — so a
//! predicate over `char` cannot be reached without first converting, and the
//! conversion is exactly what a lone surrogate refuses. Every code point in the
//! set is in the Basic Multilingual Plane and none of them is a surrogate, so a
//! unit-wise test is not an approximation: it is the same question, asked where
//! it can always be answered.
//!
//! # Why the set is written out rather than taken from `char::is_whitespace`
//!
//! They disagree, in the direction that a test suite of ASCII never notices:
//! `char::is_whitespace` is `White_Space` from the Unicode database, which
//! includes **U+0085 NEXT LINE**, and the language does not — `"\u{85}x".trim()`
//! keeps its first character in every engine. It also excludes **U+FEFF**, which
//! the language does trim. Two differences, both silent, both at the edges of
//! text a program pasted in from somewhere else.
//!
//! # The rule stated twice, and where the other one is
//!
//! `crate::coerce::number::is_string_whitespace` states this same set over
//! `char`, for the leading and trailing space `ToNumber` skips before reading
//! digits. That is the same set by the specification's own definition —
//! `StrWhiteSpaceChar` is `WhiteSpace` and `LineTerminator`, which is what
//! `TrimString` trims — so the two must never drift, and one of them should
//! call the other. It is stated here rather than fixed here because that file
//! belongs to another change in flight; folding it in is a one-line call to
//! [`is_white_space`] after widening its `char` to a unit.

/// Whether a code unit is white space or a line terminator.
///
/// The specification's `WhiteSpace` production plus `LineTerminator`, which is
/// the set `String.prototype.trim` removes — the two are separate productions
/// and one predicate, because no operation in the language trims one without
/// the other.
pub fn is_white_space(unit: u16) -> bool {
    matches!(
        unit,
        // <TAB> <LF> <VT> <FF> <CR> <SP> — the ASCII half, and the only part a
        // program written in English ever exercises.
        0x0009..=0x000D | 0x0020
        // <NBSP>, and the Unicode `Zs` category the specification names by
        // reference rather than by listing.
        | 0x00A0 | 0x1680 | 0x2000..=0x200A | 0x202F | 0x205F | 0x3000
        // <LS> and <PS>: line terminators rather than white space, and trimmed
        // by the same production.
        | 0x2028 | 0x2029
        // <ZWNBSP>, which the specification lists in `WhiteSpace` explicitly.
        // Its neighbours U+200B..U+200D are NOT in it — a zero-width space is
        // not white space, and a program that trims one is relying on an engine
        // that is wrong.
        | 0xFEFF
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_characters_that_separate_this_set_from_unicodes() {
        assert!(
            !is_white_space(0x0085),
            "U+0085 NEXT LINE is `White_Space` in Unicode and is NOT trimmed by \
             the language — the difference `char::is_whitespace` would hide"
        );
        assert!(
            is_white_space(0xFEFF),
            "and U+FEFF is trimmed by the language and is not `White_Space`"
        );
    }

    #[test]
    fn a_zero_width_space_is_not_white_space() {
        // U+200B sits one past the end of the `Zs` run this set includes, and a
        // range written as `0x2000..=0x200B` would swallow it.
        assert!(is_white_space(0x200A));
        assert!(!is_white_space(0x200B));
        assert!(!is_white_space(0x200C));
        assert!(!is_white_space(0x200D));
    }

    #[test]
    fn no_surrogate_is_white_space() {
        // What makes the unit-wise question the same question: half a character
        // is never white space, so a lone one cannot be trimmed by accident.
        assert!(!is_white_space(0xD800));
        assert!(!is_white_space(0xDFFF));
    }
}
