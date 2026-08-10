//! Turning a string into the number a layout is keyed by.
//!
//! A property key is a string, and a program compares keys constantly — every
//! property read, every shape transition, every `in`. Comparing them by content
//! walks bytes for an answer that is nearly always "no", at a frequency
//! proportional to how often code runs rather than to how many keys exist.
//!
//! # One key space, two sources
//!
//! The number a string becomes is [`rts_cranelift::shape::Key`], minted from the
//! machine's `KeyRegistry` — **the same registry the compiler mints from** when
//! it interns an identifier it read in the source.
//!
//! That is not a detail. A shape is keyed by `Key`, and there is exactly one
//! shape tree. If the compiler numbered `"a"` from one counter and the runtime
//! numbered it from another, `obj.a` compiled to a fixed slot and `obj["a"]`
//! resolved at run time would be looking up two different keys in one tree, and
//! the second would not find what the first stored. Two numbering schemes are
//! the same failure as two shape trees, one level up.
//!
//! So this table does not invent numbers. It remembers which string got which,
//! and asks the registry when it has not seen one before.
//!
//! # Probing without allocating
//!
//! Measured: `obj[computed]` was **123x** slower than `obj.field` for the same
//! property (103.38ms vs 0.84ms over 200k reads), and interning was where it
//! went — two heap allocations per lookup, *including a hit that inserts
//! nothing*. One was upstream, copying the [`Str`]'s buffer just to have an
//! owned value to intern. The other was here: the table used to be
//! `HashMap<Vec<u16>, Key>`, so every probe first did `text.units().collect()`
//! to build a `Vec<u16>` whose only job was being compared against and thrown
//! away.
//!
//! The fix keys by a hash computed by walking [`Str::units`] as an iterator —
//! nothing collected — into `HashMap<u64, Vec<Key>>`, and disambiguates a
//! bucket (collisions are rare, and more than one distinct string can share a
//! hash) by comparing against the text this table already keeps in `text` for
//! [`Interner::text`], via [`Str::same_units`]. A hit therefore touches no heap
//! at all; only a genuine miss clones `text` once, to have an owned copy for
//! that same `text` table — which was always paid on a miss and never was the
//! measured cost.
//!
//! Two alternatives were rejected:
//! - `HashMap::raw_entry`, which would probe by hash directly without a
//!   collision bucket at all. It is nightly-only, and this crate builds on
//!   stable.
//! - Keying by [`Str`] itself, deriving `Hash`/`Eq` on it directly. Rejected
//!   because `Str`'s derived `Eq` compares [`Repr`](super::Repr) — the
//!   *representation* — not the units: `Repr::Latin1(vec![b'a'])` and
//!   `Repr::Utf16(vec![0x61])` are unequal by that derive even though
//!   [`Str::same_units`] says they are one string. Deriving on `Str` as stored
//!   would silently mint two keys for one property name the moment a narrow
//!   and a wide spelling of it both showed up — worse than the allocation it
//!   would have saved.
//!
//! # Why there are still two tables
//!
//! The compiler's table maps *source identifiers* to keys and holds them for the
//! life of a compilation. This one maps *strings computed while running* — a key
//! built by `"item" + i` has no entry in the first, and never could.
//!
//! Different lifetimes, different contents, one number space. That is the shape
//! of the thing, and merging them would mean keeping source text alive for the
//! life of the process.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rts_cranelift::shape::{Key, KeyRegistry};

use super::Str;

/// The hash a string's code units probe under.
///
/// Walks [`Str::units`] as an iterator, so a probe never allocates — see the
/// module documentation for the 123x measurement this is the fix for. Two
/// strings with the same units always produce the same hash, which is what
/// makes bucketing by it in [`Interner::keys`] sound; two different strings
/// occasionally sharing one is expected and handled by the equality check
/// that follows a bucket hit.
fn units_hash(text: &Str) -> u64 {
    let mut hasher = DefaultHasher::new();
    // The length goes in too, so e.g. `[0x61, 0x00]` and `[0x6100]` — which
    // cannot both be code-unit sequences of one string, but would otherwise
    // be hashed by an unbounded stream of units alone — are not relied upon
    // to disagree by accident.
    text.len().hash(&mut hasher);
    for unit in text.units() {
        unit.hash(&mut hasher);
    }
    hasher.finish()
}

/// Every string that has been used as a property key while running.
#[derive(Default)]
pub struct Interner {
    /// The key(s) whose text hashes to a given value, keyed by that hash
    /// rather than by the text itself.
    ///
    /// Bucketed rather than one-to-one because a hash collision between two
    /// *different* strings is possible, however rare; when a bucket has more
    /// than one candidate, [`Interner::intern`] disambiguates by comparing
    /// against `text`, so correctness never depends on the hash being
    /// collision-free — only speed does.
    keys: HashMap<u64, Vec<Key>>,
    /// The text, for the places the language genuinely needs it back — and,
    /// on a bucket hit, for the equality check that confirms it rather than
    /// merely a matching hash.
    text: HashMap<Key, Str>,
}

impl Interner {
    /// An interner that has seen nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The key for a string, minting one from the registry the first time.
    ///
    /// Takes the registry rather than owning it, for the reason in the module
    /// documentation: there is one key space, and a table that owned a registry
    /// would be a second one.
    ///
    /// A hit — the common case, every re-read of an existing property —
    /// allocates nothing: the hash is computed over the borrowed `text`
    /// without collecting, and the bucket lookup borrows the already-owned
    /// [`Str`] in `text` for the comparison. Only a genuine miss clones
    /// `text`, because `text` needs an owned copy regardless of this change.
    pub fn intern(&mut self, text: &Str, registry: &mut KeyRegistry) -> Key {
        let hash = units_hash(text);
        if let Some(candidates) = self.keys.get(&hash) {
            for candidate in candidates {
                if self.text.get(candidate).is_some_and(|found| found.same_units(text)) {
                    return *candidate;
                }
            }
        }
        let key = registry.declare_one();
        self.keys.entry(hash).or_default().push(key);
        self.text.insert(key, text.clone());
        key
    }

    /// The key for Rust text.
    pub fn intern_str(&mut self, text: &str, registry: &mut KeyRegistry) -> Key {
        self.intern(&Str::from_str(text), registry)
    }

    /// What a key says, if this table is the one that minted it.
    ///
    /// For diagnostics and for the places the language genuinely needs the text
    /// — `Object.keys`, a thrown message. A *decision* that reads this is a
    /// decision that could have compared numbers and did not.
    ///
    /// Absent for a key the compiler minted, which this table never saw. That is
    /// not a failure: two tables feed one number space, and neither can name
    /// what the other interned.
    pub fn text(&self, key: Key) -> Option<&Str> {
        self.text.get(&key)
    }

    /// How many distinct strings this table has interned.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether none.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Repr;

    #[test]
    fn the_same_text_interns_to_the_same_key() {
        let mut registry = KeyRegistry::new();
        let mut interner = Interner::new();

        let first = interner.intern_str("length", &mut registry);
        assert_eq!(first, interner.intern_str("length", &mut registry));
        assert_ne!(first, interner.intern_str("size", &mut registry));
        assert_eq!(interner.len(), 2);
        assert_eq!(registry.len(), 2, "and no key was minted twice");
    }

    #[test]
    fn two_layouts_of_one_string_reach_one_key() {
        let mut registry = KeyRegistry::new();
        let mut interner = Interner::new();

        let narrow = Str::from_str("a");
        let wide = Str::from_utf16(&[0x0061, 0x65E5]);
        // The only way to build a wide "a": slice something that had to be wide.
        let wide_a = Str::from_utf16(&[wide.unit_at(0).unwrap()]);

        assert!(matches!(narrow.repr(), Repr::Latin1(_)));
        assert_eq!(
            interner.intern(&narrow, &mut registry),
            interner.intern(&wide_a, &mut registry),
            "otherwise obj[\"a\"] would miss a property stored under a \
             differently-built \"a\""
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn the_runtime_and_the_compiler_share_one_numbering() {
        let mut registry = KeyRegistry::new();
        let mut interner = Interner::new();

        // What the compiler does when it reads an identifier.
        let from_source = registry.declare_one();
        // What the runtime does with a computed string.
        let from_runtime = interner.intern_str("a", &mut registry);

        assert_ne!(
            from_source, from_runtime,
            "different strings get different keys — and both came from one \
             registry, so a shape keyed by either is reachable by either"
        );
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn a_key_this_table_did_not_mint_has_no_text_here() {
        let mut registry = KeyRegistry::new();
        let interner = Interner::new();
        let elsewhere = registry.declare_one();

        assert!(
            interner.text(elsewhere).is_none(),
            "two tables feed one number space, and neither names what the other \
             interned"
        );
    }

    #[test]
    fn an_interned_string_keeps_its_text() {
        let mut registry = KeyRegistry::new();
        let mut interner = Interner::new();

        let key = interner.intern_str("toString", &mut registry);
        assert_eq!(
            interner.text(key).and_then(Str::to_rust).as_deref(),
            Some("toString")
        );
    }

    #[test]
    fn a_lone_surrogate_can_be_a_key() {
        let mut registry = KeyRegistry::new();
        let mut interner = Interner::new();

        let odd = Str::from_utf16(&[0xD800]);
        let key = interner.intern(&odd, &mut registry);

        assert_eq!(
            interner.intern(&odd, &mut registry),
            key,
            "it is a legal string, so it is a legal property key"
        );
        assert!(interner.text(key).is_some_and(|s| !s.is_well_formed()));
    }
}
