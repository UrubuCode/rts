//! What a property is named.
//!
//! Every non-symbol property key is a **string** — `obj[0]` and `obj["0"]` are
//! the same property, because `ToPropertyKey` runs `ToString` on anything that
//! is not already a symbol.
//!
//! So why is there an [`Key::Index`] variant at all, if an index is a string?
//! Because enumeration order distinguishes them: integer-index keys come first,
//! in ascending numeric order, before every other string. A table that stored
//! them all as strings would have to re-derive which are indices on every
//! enumeration, and re-derive it with the exact rule below, which is the part
//! that is easy to get subtly wrong.

use crate::text::{Interned, Interner, Str};

/// The largest array index.
///
/// `2³² − 2`, not `2³² − 1`: the last value is reserved as the maximum `length`,
/// so an array can hold every index up to it and still have a length that fits.
/// A key of `"4294967295"` is therefore an ordinary string property, on an array
/// as much as anywhere else.
pub const MAX_ARRAY_INDEX: u32 = u32::MAX - 1;

/// What a property is named.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Key {
    /// A canonical integer index, which enumerates first and in numeric order.
    Index(u32),
    /// Any other string.
    Name(Interned),
}

/// Whether a string is an array index, and which one.
///
/// The rule is **canonical form**, not "looks numeric". A string is an array
/// index when converting it to a number and back yields the same string — so
/// each of these is an ordinary string property despite looking like a number:
///
/// | key | why not |
/// |---|---|
/// | `"01"` | a leading zero does not survive the round trip |
/// | `"1.0"` | neither does a trailing `.0` |
/// | `"+1"` | nor a sign |
/// | `"-0"` | round-trips to `"0"`, which is a different string |
/// | `"1e2"` | round-trips to `"100"` |
/// | `"4294967295"` | in range for a `u32`, out of range for an index |
///
/// Getting this wrong routes a property through the fast integer path that the
/// language says lives among the string keys, which shows up as an enumeration
/// order nobody can explain.
pub fn as_array_index(text: &Str) -> Option<u32> {
    let length = text.len();
    if length == 0 || length > 10 {
        // Ten digits is the most `MAX_ARRAY_INDEX` can take; anything longer
        // cannot be one and need not be parsed to find out.
        return None;
    }

    let first = text.unit_at(0)?;
    if !first.is_ascii_digit() {
        return None;
    }
    // "0" is an index; "0…" for any longer string is not canonical.
    if first == b'0' as u16 && length > 1 {
        return None;
    }

    let mut value: u64 = 0;
    for position in 0..length {
        let unit = text.unit_at(position)?;
        if !unit.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u64::from(unit - b'0' as u16);
        if value > u64::from(MAX_ARRAY_INDEX) {
            return None;
        }
    }

    Some(value as u32)
}

impl Key {
    /// The key a string names, choosing the index form when the string is one.
    pub fn from_str(text: &Str, interner: &mut Interner) -> Key {
        match as_array_index(text) {
            Some(index) => Key::Index(index),
            None => Key::Name(interner.intern(text)),
        }
    }

    /// Whether this key enumerates before every string key.
    pub fn is_index(self) -> bool {
        matches!(self, Key::Index(_))
    }
}

/// Extension trait spelling the ASCII-digit test on a code unit.
trait AsciiDigit {
    fn is_ascii_digit(self) -> bool;
}

impl AsciiDigit for u16 {
    fn is_ascii_digit(self) -> bool {
        (b'0' as u16..=b'9' as u16).contains(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(text: &str) -> Option<u32> {
        as_array_index(&Str::from_str(text))
    }

    #[test]
    fn a_canonical_integer_string_is_an_index() {
        assert_eq!(index_of("0"), Some(0));
        assert_eq!(index_of("1"), Some(1));
        assert_eq!(index_of("42"), Some(42));
        assert_eq!(index_of("4294967294"), Some(MAX_ARRAY_INDEX));
    }

    #[test]
    fn everything_that_merely_looks_numeric_is_a_string_key() {
        for text in ["01", "1.0", "+1", "-1", "-0", "1e2", " 1", "1 ", ""] {
            assert_eq!(
                index_of(text),
                None,
                "{text:?} does not survive the round trip, so it is an ordinary \
                 string property"
            );
        }
    }

    #[test]
    fn the_last_u32_is_out_of_range_because_length_needs_it() {
        assert_eq!(index_of("4294967294"), Some(MAX_ARRAY_INDEX));
        assert_eq!(
            index_of("4294967295"),
            None,
            "2**32 - 1 is the maximum length, not a maximum index"
        );
        assert_eq!(index_of("4294967296"), None);
        assert_eq!(index_of("99999999999"), None, "too long to be one at all");
    }

    #[test]
    fn a_key_chooses_the_index_form_only_when_the_string_is_one() {
        let mut interner = Interner::new();

        assert_eq!(
            Key::from_str(&Str::from_str("0"), &mut interner),
            Key::Index(0)
        );
        assert!(Key::from_str(&Str::from_str("01"), &mut interner).is_index() == false);
        assert!(!Key::from_str(&Str::from_str("length"), &mut interner).is_index());
    }

    #[test]
    fn a_non_ascii_digit_is_not_a_digit() {
        // Arabic-Indic digit one. It is a digit to a human and not to
        // ToString, so it is a string key.
        assert_eq!(as_array_index(&Str::from_utf16(&[0x0661])), None);
    }
}
