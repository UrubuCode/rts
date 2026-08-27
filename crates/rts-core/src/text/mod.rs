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
//!
//! ## What the neighbouring modules are for
//!
//! All three exist because of the same sentence above — a string may not be
//! valid Unicode — and each says its own half of it:
//!
//! - [`runs`] applies an operation defined over `&str` to a string that has no
//!   `&str`, by mapping the valid runs and carrying the unpaired surrogates
//!   between them. Case mapping and normalisation both go through it.
//! - [`normalize`] is the four Unicode forms, and says why a decomposition
//!   table is carried here where a collation table is refused.
//! - [`space`] is which code units the language calls white space, asked one
//!   unit at a time so that a string with half a character can still be
//!   trimmed.

mod intern;
mod normalize;
mod runs;
mod space;

pub use intern::Interner;
pub use normalize::{Form, normalized};
pub use runs::mapped_runs;
pub use space::is_white_space;

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

/// The code units of a string, in whichever layout holds them.
///
/// # Why this is an enum and was a `Box<dyn Iterator>`
///
/// Because it is the front door. Eighteen call sites walk a string through
/// this, and a boxed trait object costs a heap allocation to build and an
/// indirect call PER CODE UNIT to advance — on the hot path of `indexOf`,
/// `slice`, `split`, `toUpperCase` and the interner's hash, which the interner
/// documents as "a hit therefore touches no heap at all" and which was
/// therefore false.
///
/// Two variants and a match per `next` is what replaces it: no allocation, and
/// a call the compiler can see through.
///
/// The narrow arm still widens each byte, because a code unit is what every
/// caller asked for. A caller that can work on BYTES should ask [`Str::narrow`]
/// instead and skip this entirely — which is the larger win and is available
/// only to callers that know they do not need the wide form.
pub enum Units<'a> {
    /// One byte per code unit, widened as it is read.
    Narrow(core::slice::Iter<'a, u8>),
    /// Two bytes per code unit.
    Wide(core::slice::Iter<'a, u16>),
}

impl Iterator for Units<'_> {
    type Item = u16;

    fn next(&mut self) -> Option<u16> {
        match self {
            Units::Narrow(bytes) => bytes.next().map(|byte| u16::from(*byte)),
            Units::Wide(units) => units.next().copied(),
        }
    }

    /// Exact, and given so that a caller collecting into a `Vec` allocates once.
    ///
    /// The boxed form could not report one — a trait object's `size_hint` is
    /// whatever the erased type said, and nothing here was asked. Every
    /// `units().collect()` in this crate was therefore growing a vector by
    /// doubling.
    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = match self {
            Units::Narrow(bytes) => bytes.len(),
            Units::Wide(units) => units.len(),
        };
        (left, Some(left))
    }
}

impl ExactSizeIterator for Units<'_> {}

/// A string.
/// # Why `PartialEq`, `Eq` and `Hash` are written out
///
/// Because [`Str::key`] must not take part in any of them. It is a memo of
/// something derived from the text, so two strings that are equal are equal
/// whether or not either has been asked for its key yet — and a derived `Hash`
/// that included it would put the same text in two buckets depending on whether
/// anything had looked at it.
///
/// This is the shape V8 uses for the same field: a string's hash is computed
/// lazily into its header and is not part of what makes two strings the same.
#[derive(Clone, Debug)]
pub struct Str {
    repr: Repr,
    /// The property key this text resolves to, remembered after the first ask.
    ///
    /// Zero means "not asked yet"; anything else is the key's number plus one.
    ///
    /// # What it is for
    ///
    /// `o[k]` where `k` is a string reaches `Context::key_of_text_cell`, which
    /// ended in `interner.intern(text, …)` — **a hash of the text, on every
    /// access**. That made a property read cost three nanoseconds per character
    /// of the NAME, which is not a thing a property read has any business doing.
    /// Measured 2026-08-23, before this field existed:
    ///
    /// | read | ns |
    /// |---|---:|
    /// | `o.a`, a literal key | 28 |
    /// | `o[k]`, one character | 115 |
    /// | `o[k]`, 64 characters | 331 |
    /// | `o[k]`, 256 characters | 891 |
    ///
    /// # Why it lives here and not on the cell
    ///
    /// A string is a region CELL pointing at this `Str` in a slab, and the first
    /// attempt put the memo in one of the cell's payload slots. It was reverted
    /// on a diagnosis that turned out to be **wrong**, and the correction is
    /// recorded rather than quietly dropped: the corruption blamed on it —
    /// `typeof ks.map` answering `"unknown"` after sixty thousand allocations —
    /// happens on a CLEAN tree in a debug build, and is documented in
    /// `docs/engine/property-keys.md` as a separate, pre-existing defect.
    ///
    /// So the argument for this placement is not "the other one broke". It is
    /// that a payload slot already has two owners — the collector walks it as a
    /// possible reference, and a shape assigns properties to slots — while a
    /// `Str` has none. It is allocated and freed with the string it belongs to,
    /// so a memo on it cannot outlive its text and cannot be reached by a scan.
    /// It is the same place V8 keeps a string's hash and JavaScriptCore keeps a
    /// `StringImpl`'s.
    ///
    /// # Why a `Cell`
    ///
    /// Because resolving happens through a shared reference: `key_of_text_cell`
    /// holds the text out of `Context::cells` while it borrows the interner and
    /// the key registry mutably, which are different fields of the same struct.
    /// Asking for the slab mutably instead would collide with that borrow —
    /// the same shape `symbol::key_of` records for its own memo.
    key: std::cell::Cell<u32>,
    /// The SameValueZero hash used by Map/Set, stored as hash plus one.
    ///
    /// Zero means "not computed"; the real hash is masked to 31 bits, so adding
    /// one is lossless and leaves every possible value distinguishable from the
    /// sentinel. Cloning a `Str` copies this memo along with its immutable bytes.
    hash: std::cell::Cell<u32>,
}

impl PartialEq for Str {
    fn eq(&self, other: &Self) -> bool {
        self.repr == other.repr
    }
}

impl Eq for Str {}

impl std::hash::Hash for Str {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.repr.hash(state);
    }
}

impl Str {
    /// A string over a representation, with no key resolved yet.
    ///
    /// Every construction goes through here so that the memo has exactly one
    /// initial value and a thirteenth construction site cannot forget it.
    fn of(repr: Repr) -> Self {
        Str {
            repr,
            key: std::cell::Cell::new(0),
            hash: std::cell::Cell::new(0),
        }
    }

    /// The FNV-1a hash over UTF-16 code units, memoized on the immutable string.
    pub(crate) fn hash_code(&self) -> u32 {
        if let Some(cached) = self.hash.get().checked_sub(1) {
            return cached;
        }
        let mut hash: u32 = 2_166_136_261;
        for unit in self.units() {
            hash ^= u32::from(unit);
            hash = hash.wrapping_mul(16_777_619);
        }
        let hash = hash & 0x7fff_ffff;
        self.hash.set(hash + 1);
        hash
    }

    /// The property key this text was last resolved to, if it has been.
    pub(crate) fn remembered_key(&self) -> Option<u32> {
        match self.key.get() {
            0 => None,
            found => Some(found - 1),
        }
    }

    /// Remembers the key this text resolves to.
    ///
    /// Sound because a `Str` is immutable once built: nothing here can make the
    /// text disagree with the key it was resolved to, which is the same argument
    /// that lets a string cell carry its own length.
    pub(crate) fn remember_key(&self, number: u32) {
        self.key.set(number + 1);
    }

    /// The empty string.
    pub fn empty() -> Self {
        Str::of(Repr::Latin1(Vec::new()))
    }

    /// A string from Rust text.
    ///
    /// Chooses the narrow form when it fits. The check is a scan, and it is paid
    /// once at construction rather than on every read afterwards — which is the
    /// right side of that trade for text that is read more often than it is
    /// made.
    pub fn from_str(text: &str) -> Self {
        // ASCII first, and it is worth being separate from the Latin-1 arm
        // below rather than folded into it. Both of those walk the string a
        // CHARACTER at a time — once to decide and once to convert — where for
        // ASCII the decision is a SIMD scan and the conversion is a memcpy,
        // because the bytes already are the code units.
        //
        // This runs on every string a program creates: every piece a `split`
        // produces, every result of a `replace`, every key `JSON.parse` reads.
        if text.is_ascii() {
            return Str::of(Repr::Latin1(text.as_bytes().to_vec()));
        }
        if text.chars().all(|c| (c as u32) < 256) {
            return Str::of(Repr::Latin1(text.chars().map(|c| c as u8).collect()));
        }
        Str::of(Repr::Utf16(text.encode_utf16().collect()))
    }

    /// A string from bytes that are already one code unit each.
    ///
    /// No scan: `from_str` and `from_utf16` both walk their input to decide
    /// whether the narrow form fits, and a caller holding a slice OF a narrow
    /// string already knows it does. Every byte of one is below 256 by
    /// construction, so the question is answered before it is asked.
    pub fn from_latin1(bytes: &[u8]) -> Self {
        Self::owning_latin1(bytes.to_vec())
    }

    /// The same, taking bytes the caller already owns.
    ///
    /// Exists because the copy was being paid twice: a method that maps over a
    /// string collects into a `Vec` and then handed the slice to
    /// [`Self::from_latin1`], which copied it again. Two allocations of the
    /// same bytes, on every `toUpperCase`, every `slice` and every other
    /// method that builds narrow text.
    pub fn owning_latin1(bytes: Vec<u8>) -> Self {
        Str::of(Repr::Latin1(bytes))
    }

    /// A string from an already-owned wide buffer.
    ///
    /// The caller has already established that at least one code unit does not
    /// fit Latin-1, so scanning and copying the buffer again would only repeat a
    /// decision it has already made. The UTF-16 units remain authoritative,
    /// including lone surrogates.
    pub fn owning_utf16(units: Vec<u16>) -> Self {
        Str::of(Repr::Utf16(units))
    }

    /// A string from UTF-16 code units, narrowing when they all fit.
    ///
    /// Takes `u16` rather than `char` because the input may contain a lone
    /// surrogate, and refusing one would refuse a value the language produces.
    pub fn from_utf16(units: &[u16]) -> Self {
        if units.iter().all(|unit| *unit < 256) {
            return Str::of(Repr::Latin1(units.iter().map(|unit| *unit as u8).collect()));
        }
        Str::of(Repr::Utf16(units.to_vec()))
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
    pub fn units(&self) -> Units<'_> {
        match &self.repr {
            Repr::Latin1(bytes) => Units::Narrow(bytes.iter()),
            Repr::Utf16(units) => Units::Wide(units.iter()),
        }
    }

    /// The bytes, when every code unit fits in one.
    ///
    /// For the callers that never needed to widen: a search, a comparison, a
    /// copy. `None` for the wide form, which is the honest answer rather than a
    /// re-encoding — a caller that gets `None` has to handle two layouts, and
    /// that is the trade this representation makes everywhere else too.
    pub fn narrow(&self) -> Option<&[u8]> {
        match &self.repr {
            Repr::Latin1(bytes) => Some(bytes),
            Repr::Utf16(_) => None,
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

    /// Whether this begins with an ASCII prefix, without building anything.
    ///
    /// # Why this exists rather than `to_rust().starts_with(…)`
    ///
    /// Because that was the spelling, and it allocated a whole `String` copy of
    /// the subject to look at its first two bytes. Enumeration asks it ONCE PER
    /// KEY — `Object.keys` filters symbol-keyed properties out by exactly this
    /// test — so a four-property object allocated four strings it read two
    /// bytes of and dropped.
    ///
    /// ASCII-only in the parameter and that is the point: a JavaScript string
    /// is UTF-16 units, and a prefix outside ASCII would need to know how the
    /// two representations line up. Every caller here spells an ASCII marker,
    /// so the comparison is unit-against-byte and no encoding question arises.
    ///
    /// # Panics
    ///
    /// Never. A non-ASCII `prefix` answers `false` rather than asserting: this
    /// is a predicate, and a caller that passes one is asking whether a UTF-16
    /// string starts with something this cannot represent, which it does not.
    pub fn starts_with_ascii(&self, prefix: &str) -> bool {
        if !prefix.is_ascii() {
            return false;
        }
        let bytes = prefix.as_bytes();
        match &self.repr {
            Repr::Latin1(held) => held.starts_with(bytes),
            Repr::Utf16(units) => {
                units.len() >= bytes.len()
                    && units
                        .iter()
                        .zip(bytes)
                        .all(|(unit, byte)| *unit == u16::from(*byte))
            }
        }
    }

    /// Rust text, if this is well-formed.
    ///
    /// Absent for a lone surrogate rather than replaced with `U+FFFD`. A
    /// replacement character is a different string, and returning one silently
    /// turns "this cannot be represented in Rust" into "this is what it said".
    pub fn to_rust(&self) -> Option<String> {
        match &self.repr {
            Repr::Latin1(bytes) => match bytes.is_ascii() {
                // ASCII bytes ARE UTF-8 bytes, so this is a copy and nothing
                // else. The general arm below builds the string one `char` at a
                // time — correct for Latin-1, where a byte above 127 becomes
                // two UTF-8 bytes, and enormously wasteful for the ASCII that
                // nearly all program text is: `String::split` was measured at
                // 3.2 ns PER CHARACTER of its subject, linear in the subject and
                // independent of how many pieces came out, which is this.
                //
                // `is_ascii` is a SIMD scan in std and `from_utf8` re-validates
                // with another; both are memory-bandwidth work against a
                // per-character loop.
                true => String::from_utf8(bytes.to_vec()).ok(),
                false => Some(bytes.iter().map(|byte| char::from(*byte)).collect()),
            },
            Repr::Utf16(units) => String::from_utf16(units).ok(),
        }
    }

    /// Rust text, with each lone surrogate written as `U+FFFD`.
    ///
    /// # Why this exists beside [`Self::to_rust`] instead of replacing it
    ///
    /// The two answer different questions and both are asked. `to_rust` is
    /// "give me this string, or tell me you cannot" — `normalize`, `RegExp` and
    /// `Buffer` all need the refusal, because operating on a replacement
    /// character silently produces a result for text the program never wrote.
    ///
    /// This one is "write this string somewhere that speaks UTF-8", which has no
    /// refusal available: stdout is bytes, and every engine encodes a lone
    /// surrogate as the replacement character on the way out. Answering `None`
    /// there is what made `console.log(emoji.charAt(0))` print an OBJECT — the
    /// inspector took the absence as "not a string" and fell through to dumping
    /// indices.
    pub fn to_rust_lossy(&self) -> String {
        match &self.repr {
            // Latin-1 has no surrogates in it, so the two agree here and there
            // is nothing to replace — but the conversion is still the cheap
            // ASCII path when it applies, which is why it defers rather than
            // decoding a second way.
            Repr::Latin1(_) => self.to_rust().unwrap_or_default(),
            Repr::Utf16(units) => String::from_utf16_lossy(units),
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
                Str::of(Repr::Latin1(bytes))
            }
            _ => {
                let mut units = Vec::with_capacity(self.len() + other.len());
                units.extend(self.units());
                units.extend(other.units());
                Str::of(Repr::Utf16(units))
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
        assert_eq!(
            lone.to_rust_lossy(),
            "\u{FFFD}",
            "the other conversion has no refusal available: writing this to a \
             UTF-8 stream is what every runtime spells as the replacement \
             character"
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
        let wide = Str::of(Repr::Utf16(vec![b'a' as u16]));

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

    #[test]
    fn the_memoized_hash_is_identical_across_string_layouts() {
        let narrow = Str::from_str("a");
        let wide = Str::of(Repr::Utf16(vec![0x0061]));
        assert_eq!(narrow.hash_code(), wide.hash_code());
        assert_eq!(narrow.hash.get(), wide.hash.get());
    }

    #[test]
    fn the_memoized_hash_matches_the_utf16_fnv_algorithm() {
        let text = Str::from_utf16(&[0x0061, 0x00e9, 0x65e5, 0xd800]);
        let mut expected = 2_166_136_261u32;
        for unit in text.units() {
            expected ^= u32::from(unit);
            expected = expected.wrapping_mul(16_777_619);
        }
        expected &= 0x7fff_ffff;
        assert_eq!(text.hash_code(), expected);
        assert_eq!(text.hash.get(), expected + 1);
    }
}
