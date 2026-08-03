//! Strings.
//!
//! # The decision this module is built around
//!
//! **A JavaScript string is a sequence of UTF-16 code units.** Not characters,
//! not bytes, not scalar values. `length` counts code units, `charCodeAt`
//! returns one, indexing addresses one, and every oddity around emoji and
//! surrogate pairs follows from that single fact rather than from anything
//! implementations chose.
//!
//! It also means a string can hold text that is **not valid Unicode** — a lone
//! surrogate is a perfectly legal JavaScript string, produced by
//! `"\u{1F600}"[0]` and by `String.fromCharCode(0xD800)`. A representation that
//! could not hold one would be unable to represent the result of an operation
//! the language performs.
//!
//! ## Why not UTF-8
//!
//! It is what Rust has and it is the wrong shape here. `s.length` becomes a scan
//! or a lie, `s[i]` becomes a scan, and a lone surrogate cannot be stored at
//! all. Converting at every boundary trades those costs for a copy on every
//! call, which is worse in the case that matters — text that is read far more
//! often than it is created.
//!
//! ## Why not UTF-16 either, exactly
//!
//! Most strings are ASCII, and storing ASCII in sixteen bits doubles the memory
//! of nearly all text a program touches. So a string is stored in whichever of
//! two forms fits and remembers which: [`Repr::Latin1`] when every code unit is
//! below 256, [`Repr::Utf16`] otherwise.
//!
//! The narrow form is not an optimisation bolted on afterwards — it changes what
//! a length or an index costs, so every operation here is written knowing there
//! are two layouts, and none of them converts one to the other to avoid thinking
//! about it.

mod intern;

pub use intern::{Interned, Interner};

/// How a string's code units are stored.
///
/// Two layouts, one meaning: both represent a sequence of UTF-16 code units, and
/// the narrow one is available exactly when every unit fits in a byte.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Repr {
    /// One byte per code unit, for text whose every unit is below 256.
    ///
    /// Not "ASCII" and not "latin-1 the encoding" — it is the *first 256 UTF-16
    /// code units*, which happen to coincide with latin-1. A byte here is a code
    /// unit, so length and indexing are the same operations as in the wide form
    /// at half the memory.
    Latin1(Vec<u8>),
    /// Two bytes per code unit.
    ///
    /// Holds anything, including a lone surrogate, which is why the type is
    /// `u16` rather than `char`.
    Utf16(Vec<u16>),
}

/// A string.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Str {
    repr: Repr,
}

impl Str {
    /// The empty string.
    pub fn empty() -> Self {
        Str {
            repr: Repr::Latin1(Vec::new()),
        }
    }

    /// A string from Rust text.
    ///
    /// Chooses the narrow form when it fits. The check is a scan, and it is paid
    /// once at construction rather than on every read afterwards — which is the
    /// right side of that trade for text that is read more often than it is
    /// made.
    pub fn from_str(text: &str) -> Self {
        if text.chars().all(|c| (c as u32) < 256) {
            return Str {
                repr: Repr::Latin1(text.chars().map(|c| c as u8).collect()),
            };
        }
        Str {
            repr: Repr::Utf16(text.encode_utf16().collect()),
        }
    }

    /// A string from UTF-16 code units, narrowing when they all fit.
    ///
    /// Takes `u16` rather than `char` because the input may contain a lone
    /// surrogate, and refusing one would refuse a value the language produces.
    pub fn from_utf16(units: &[u16]) -> Self {
        if units.iter().all(|unit| *unit < 256) {
            return Str {
                repr: Repr::Latin1(units.iter().map(|unit| *unit as u8).collect()),
            };
        }
        Str {
            repr: Repr::Utf16(units.to_vec()),
        }
    }

    /// How it is stored.
    pub fn repr(&self) -> &Repr {
        &self.repr
    }

    /// `.length` — the number of UTF-16 code units.
    ///
    /// Constant time in both layouts, which is the whole reason for storing code
    /// units rather than bytes.
    pub fn len(&self) -> usize {
        match &self.repr {
            Repr::Latin1(bytes) => bytes.len(),
            Repr::Utf16(units) => units.len(),
        }
    }

    /// Whether it has no code units.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `.charCodeAt(index)` — one code unit, or nothing past the end.
    pub fn unit_at(&self, index: usize) -> Option<u16> {
        match &self.repr {
            Repr::Latin1(bytes) => bytes.get(index).map(|byte| u16::from(*byte)),
            Repr::Utf16(units) => units.get(index).copied(),
        }
    }

    /// Every code unit, in order.
    pub fn units(&self) -> Box<dyn Iterator<Item = u16> + '_> {
        match &self.repr {
            Repr::Latin1(bytes) => Box::new(bytes.iter().map(|byte| u16::from(*byte))),
            Repr::Utf16(units) => Box::new(units.iter().copied()),
        }
    }

    /// Whether this holds text that is valid Unicode.
    ///
    /// False for a string containing a lone surrogate — which is legal, and
    /// which is why this is a question rather than an invariant.
    pub fn is_well_formed(&self) -> bool {
        match &self.repr {
            // Every value below 256 is a valid scalar; the surrogate range
            // starts far above it.
            Repr::Latin1(_) => true,
            Repr::Utf16(units) => char::decode_utf16(units.iter().copied()).all(|r| r.is_ok()),
        }
    }

    /// Rust text, if this is well-formed.
    ///
    /// Absent for a lone surrogate rather than replaced with `U+FFFD`. A
    /// replacement character is a different string, and returning one silently
    /// turns "this cannot be represented in Rust" into "this is what it said".
    pub fn to_rust(&self) -> Option<String> {
        match &self.repr {
            Repr::Latin1(bytes) => Some(bytes.iter().map(|byte| char::from(*byte)).collect()),
            Repr::Utf16(units) => String::from_utf16(units).ok(),
        }
    }

    /// Two strings joined.
    ///
    /// The result narrows when both sides are narrow, and widens otherwise.
    /// Concatenating ASCII a thousand times therefore stays narrow, rather than
    /// widening once and never coming back.
    pub fn concat(&self, other: &Str) -> Str {
        match (&self.repr, &other.repr) {
            (Repr::Latin1(left), Repr::Latin1(right)) => {
                let mut bytes = Vec::with_capacity(left.len() + right.len());
                bytes.extend_from_slice(left);
                bytes.extend_from_slice(right);
                Str {
                    repr: Repr::Latin1(bytes),
                }
            }
            _ => {
                let mut units = Vec::with_capacity(self.len() + other.len());
                units.extend(self.units());
                units.extend(other.units());
                Str {
                    repr: Repr::Utf16(units),
                }
            }
        }
    }

    /// Whether two strings hold the same code units.
    ///
    /// Compares by content across layouts: a narrow `"a"` and a wide `"a"` are
    /// the same string, and a comparison that returned false for them would make
    /// equality depend on how a string was built.
    pub fn same_units(&self, other: &Str) -> bool {
        if self.len() != other.len() {
            return false;
        }
        match (&self.repr, &other.repr) {
            (Repr::Latin1(left), Repr::Latin1(right)) => left == right,
            (Repr::Utf16(left), Repr::Utf16(right)) => left == right,
            _ => self.units().eq(other.units()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_stored_narrow_and_anything_else_is_not() {
        assert!(matches!(Str::from_str("hello").repr(), Repr::Latin1(_)));
        assert!(matches!(Str::from_str("café").repr(), Repr::Latin1(_)));
        assert!(
            matches!(Str::from_str("日本").repr(), Repr::Utf16(_)),
            "above 255, so the narrow form cannot hold it"
        );
    }

    #[test]
    fn length_counts_code_units_and_not_characters() {
        // One emoji is one scalar and TWO UTF-16 code units, which is why
        // "😀".length is 2 in every JavaScript engine.
        let emoji = Str::from_str("😀");
        assert_eq!(emoji.len(), 2);
        assert_eq!(Str::from_str("a😀").len(), 3);
    }

    #[test]
    fn a_lone_surrogate_is_a_legal_string() {
        let lone = Str::from_utf16(&[0xD800]);
        assert_eq!(lone.len(), 1);
        assert_eq!(lone.unit_at(0), Some(0xD800));
        assert!(
            !lone.is_well_formed(),
            "it is not valid Unicode, and it is still a string the language makes"
        );
        assert_eq!(
            lone.to_rust(),
            None,
            "absent rather than U+FFFD — a replacement character is a different \
             string"
        );
    }

    #[test]
    fn indexing_an_emoji_yields_half_of_it() {
        let emoji = Str::from_str("😀");
        let first = Str::from_utf16(&[emoji.unit_at(0).unwrap()]);

        assert!(
            !first.is_well_formed(),
            "\"😀\"[0] is a lone high surrogate — the behaviour that makes \
             UTF-16 code units the thing being stored"
        );
    }

    #[test]
    fn concatenating_narrow_text_stays_narrow() {
        let mut built = Str::empty();
        for _ in 0..100 {
            built = built.concat(&Str::from_str("ab"));
        }
        assert_eq!(built.len(), 200);
        assert!(
            matches!(built.repr(), Repr::Latin1(_)),
            "widening once and never narrowing again is how ASCII text ends up \
             costing twice its size"
        );
    }

    #[test]
    fn concatenating_across_layouts_widens() {
        let joined = Str::from_str("a").concat(&Str::from_str("日"));
        assert!(matches!(joined.repr(), Repr::Utf16(_)));
        assert_eq!(joined.to_rust().as_deref(), Some("a日"));
    }

    #[test]
    fn equality_does_not_depend_on_how_a_string_was_built() {
        let narrow = Str::from_str("a");
        let wide = Str {
            repr: Repr::Utf16(vec![b'a' as u16]),
        };

        assert_ne!(narrow.repr(), wide.repr(), "stored differently");
        assert!(
            narrow.same_units(&wide),
            "and they are the same string, because a string is its code units"
        );
    }

    #[test]
    fn the_empty_string_is_empty_in_both_directions() {
        let empty = Str::empty();
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert_eq!(empty.unit_at(0), None);
        assert_eq!(empty.to_rust().as_deref(), Some(""));
    }
}
