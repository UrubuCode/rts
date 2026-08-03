//! The spellings of the states that are not written as digits.
//!
//! # Why these are constants and not literals
//!
//! `NaN`, `Infinity` and `-Infinity` are produced by `ToString` and recognised
//! by `ToNumber`, so each of them is written in two places that must agree. Two
//! literals agree until one is edited, and the failure is quiet: a number that
//! prints one way and reads back as something else.
//!
//! `undefined` and `null` are here for a stronger reason. They are **not
//! numbers**, and nothing in this module may turn their spelling into a value.
//! `Number("undefined")` is `NaN` — the word is a string that fails to be a
//! number, not a name for the singleton. Keeping the spelling in one place, with
//! this note on it, is what stops someone adding a helpful special case that
//! turns the string `"undefined"` into the encoded `undefined`.
//!
//! # The states themselves live in two different places, and neither is here
//!
//! `NaN` and the infinities are `f64` values — the machine owns their bit
//! patterns, and `CANONICAL_NAN` is the one that matters.
//!
//! `undefined` and `null` are **singletons the language declares**: the machine's
//! `TagRegistry` says the singleton space "is entirely the client's" and numbers
//! nothing itself. So the numbers belong to whoever declared them and reach this
//! crate as [`crate::coerce::Singletons`] — passed in, never assumed, because a
//! second language on this machine numbers its own differently.
//!
//! What is here is only how they are *spelled*.

/// How `ToString` writes a `NaN`, and the only spelling `ToNumber` recognises.
///
/// Case-sensitive: `Number("nan")` is `NaN`, and so is `Number("NaN")` — the
/// first because it is not a number, the second because it *is* the number
/// `NaN`. Those are different reasons for the same answer, which is why a test
/// asserting `is_nan()` on either proves nothing on its own.
pub const NAN: &str = "NaN";

/// How `ToString` writes a positive infinity.
pub const INFINITY: &str = "Infinity";

/// How `ToString` writes a negative infinity.
pub const NEGATIVE_INFINITY: &str = "-Infinity";

/// The word `undefined`.
///
/// A string, and never a value. Present so that no code reaches for the literal
/// and, while it is there, decides to be helpful about it.
pub const UNDEFINED: &str = "undefined";

/// The word `null`. A string, and never a value — see [`UNDEFINED`].
pub const NULL: &str = "null";

/// Whether a string is one of the spellings that is **not** a number.
///
/// `"undefined"` and `"null"` both convert to `NaN`, and this exists so a test
/// can say *which* of the two reasons applies: they are refused as words, not
/// converted into the singletons they name.
pub fn is_non_numeric_word(text: &str) -> bool {
    text == UNDEFINED || text == NULL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_that_name_singletons_are_not_numbers() {
        assert!(is_non_numeric_word(UNDEFINED));
        assert!(is_non_numeric_word(NULL));
        assert!(!is_non_numeric_word(NAN), "NaN IS a number, spelled");
        assert!(!is_non_numeric_word(INFINITY));
    }

    #[test]
    fn the_negative_infinity_spelling_is_the_positive_one_signed() {
        assert_eq!(NEGATIVE_INFINITY, format!("-{INFINITY}"));
    }
}
