//! One number per distinct string.
//!
//! A property key is a string, and a program compares keys constantly — every
//! property read, every shape transition, every `in`. Comparing them by content
//! means walking bytes for an answer that is nearly always "no", at a frequency
//! proportional to how often code runs rather than to how many keys exist.
//!
//! So a string used as a key is interned once and compared as a number
//! afterwards. This is the same shape as the language layer's `Name`, and
//! deliberately a different table: that one interns identifiers *while
//! compiling*, this one interns strings *while running*, and a program that
//! computes a key at run time has no entry in the first.

use std::collections::HashMap;

use super::Str;

/// An interned string.
///
/// Two are the same string when their numbers match. Comparing the text of two
/// of these is what interning exists to stop happening.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct Interned(pub u32);

/// Every string that has been used as a key.
#[derive(Default)]
pub struct Interner {
    /// Interned text, indexed by number.
    text: Vec<Str>,
    /// The reverse, for finding an existing number.
    ///
    /// Keyed by the code units rather than by [`Str`] so that a narrow and a
    /// wide spelling of the same text find each other. Two layouts of one string
    /// must intern to one number, or `obj["a"]` would miss a property stored
    /// under a differently-built `"a"`.
    numbers: HashMap<Vec<u16>, Interned>,
}

impl Interner {
    /// An interner that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The number for a string, assigning one the first time.
    pub fn intern(&mut self, text: &Str) -> Interned {
        let units: Vec<u16> = text.units().collect();
        if let Some(existing) = self.numbers.get(&units) {
            return *existing;
        }
        let number = Interned(self.text.len() as u32);
        self.text.push(text.clone());
        self.numbers.insert(units, number);
        number
    }

    /// The number for Rust text.
    pub fn intern_str(&mut self, text: &str) -> Interned {
        self.intern(&Str::from_str(text))
    }

    /// What an interned string says.
    ///
    /// For diagnostics and for the places the language genuinely needs the text
    /// — `Object.keys`, a thrown message. A *decision* that reads this is a
    /// decision that could have compared numbers and did not.
    pub fn text(&self, interned: Interned) -> Option<&Str> {
        self.text.get(interned.0 as usize)
    }

    /// How many distinct strings have been interned.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether none have.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Repr;

    #[test]
    fn the_same_text_interns_to_the_same_number() {
        let mut interner = Interner::new();
        assert_eq!(interner.intern_str("length"), interner.intern_str("length"));
        assert_ne!(interner.intern_str("length"), interner.intern_str("size"));
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn two_layouts_of_one_string_intern_to_one_number() {
        let mut interner = Interner::new();

        let narrow = Str::from_str("a");
        let wide = Str::from_utf16(&[0x0061, 0x65E5]);
        // Build a wide "a" the only way the narrowing constructor allows: by
        // taking a slice of something that had to be wide.
        let wide_a = Str::from_utf16(&[wide.unit_at(0).unwrap()]);

        assert!(matches!(narrow.repr(), Repr::Latin1(_)));
        assert_eq!(
            interner.intern(&narrow),
            interner.intern(&wide_a),
            "otherwise obj[\"a\"] would miss a property stored under a \
             differently-built \"a\""
        );
        assert_eq!(interner.len(), 1);
    }

    #[test]
    fn an_interned_string_keeps_its_text() {
        let mut interner = Interner::new();
        let key = interner.intern_str("toString");
        assert_eq!(
            interner.text(key).and_then(Str::to_rust).as_deref(),
            Some("toString")
        );
        assert!(interner.text(Interned(99)).is_none());
    }

    #[test]
    fn a_lone_surrogate_can_be_a_key() {
        let mut interner = Interner::new();
        let odd = Str::from_utf16(&[0xD800]);
        let key = interner.intern(&odd);

        assert_eq!(
            interner.intern(&odd),
            key,
            "it is a legal string, so it is a legal property key"
        );
        assert!(interner.text(key).is_some_and(|s| !s.is_well_formed()));
    }
}
