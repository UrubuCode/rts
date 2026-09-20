//! Narrow text that is short enough not to be anywhere else.
//!
//! # Why this exists, and what it is instead of
//!
//! Every string this runtime makes was a `Vec<u8>`: one trip to the allocator to
//! be born and another to die. Most strings a program makes are short — a
//! property key, a number's digits, a word out of a `split`, a field of a parsed
//! document — and for those the trips ARE the cost.
//!
//! Priced rather than assumed, by `examples/alloc_cost` on 2026-09-19: making
//! the two-character text of `String(42)` inside the runtime cost about 90 ns,
//! of which **33 was the `Vec`'s malloc and free** and 27 the heap cell's own
//! life. A third of every string, to hold two bytes.
//!
//! So the bytes of a short string live IN the string, up to [`INLINE`] of them.
//! Cloning one copies thirty-two bytes and dropping one does nothing.
//!
//! # What this is instead of, and why the other one lost
//!
//! Swapping the process allocator. That was measured too — mimalloc took
//! `JSON.parse` of a hundred-row document from 120 µs to 85 — and it was not
//! taken. It puts a C toolchain under every build of this workspace and under
//! the AOT archive, which fails rule 1's availability question; and it treats
//! the symptom, because a quicker allocation is still an allocation. `memchr`
//! is the precedent this crate already keeps: not a faster loop around the same
//! work, less work.
//!
//! # Why it costs no space at all
//!
//! Measured with `size_of` before it was written, because the reasoning said
//! otherwise: a `Vec<u8>` is twenty-four bytes and an enum holding one is
//! thirty-two, so `Repr::Latin1(Narrow)` looked like forty. It is
//! **thirty-two** — the compiler puts `Repr`'s discriminant in a niche of this
//! type — so `Str` stays at forty bytes and nothing pays for the inline form
//! but the strings that use it.
//!
//! # Why equality is over the bytes and not derived
//!
//! A derive would compare variants, so an inline `"a"` and a spilled `"a"` would
//! differ — and `Str`'s own `Eq` and `Hash` sit on top of this. Nothing here
//! builds a short string spilled, but a rule that holds only while every future
//! constructor remembers it is the kind this crate keeps losing. Comparing the
//! slices makes the question unaskable.

/// The most bytes a [`Narrow`] holds without allocating.
///
/// Thirty: the space the `Long` variant needs anyway, less this enum's own
/// discriminant and the length. Raising it grows every `Str` in the program and
/// `it_costs_the_space_a_vec_and_a_tag_already_did` is what says so.
pub const INLINE: usize = 30;

/// Latin-1 text: one byte per code unit, inline when it is short.
#[derive(Clone)]
pub enum Narrow {
    /// At most [`INLINE`] bytes, held here. Bytes past `len` are zero, which
    /// nothing reads and which keeps a copy of one deterministic.
    Short {
        /// How many of `held` are text.
        len: u8,
        /// The text, then zeros.
        held: [u8; INLINE],
    },
    /// Anything longer, on the heap as it always was.
    Long(Vec<u8>),
}

impl Narrow {
    /// No text.
    pub const fn new() -> Self {
        Narrow::Short { len: 0, held: [0; INLINE] }
    }

    /// A copy of `bytes`, inline when it fits.
    pub fn from_slice(bytes: &[u8]) -> Self {
        if bytes.len() > INLINE {
            return Narrow::Long(bytes.to_vec());
        }
        let mut held = [0; INLINE];
        held[..bytes.len()].copy_from_slice(bytes);
        Narrow::Short { len: bytes.len() as u8, held }
    }

    /// Bytes the caller already owns. Short ones are copied in and the buffer is
    /// given back to the allocator, which is the better half of the trade: the
    /// string may be cloned and kept for the life of the program, and the buffer
    /// was going to be freed exactly once either way.
    pub fn from_vec(bytes: Vec<u8>) -> Self {
        if bytes.len() > INLINE {
            return Narrow::Long(bytes);
        }
        Narrow::from_slice(&bytes)
    }

    /// Two runs joined, with no buffer in between when the result is short.
    pub fn joined(left: &[u8], right: &[u8]) -> Self {
        let total = left.len() + right.len();
        if total > INLINE {
            let mut bytes = Vec::with_capacity(total);
            bytes.extend_from_slice(left);
            bytes.extend_from_slice(right);
            return Narrow::Long(bytes);
        }
        let mut held = [0; INLINE];
        held[..left.len()].copy_from_slice(left);
        held[left.len()..total].copy_from_slice(right);
        Narrow::Short { len: total as u8, held }
    }

    /// Code units known to be below 256 each, narrowed as they are copied.
    pub fn from_units(units: &[u16]) -> Self {
        if units.len() > INLINE {
            return Narrow::Long(units.iter().map(|unit| *unit as u8).collect());
        }
        let mut held = [0; INLINE];
        for (byte, unit) in held.iter_mut().zip(units) {
            *byte = *unit as u8;
        }
        Narrow::Short { len: units.len() as u8, held }
    }

    /// The text.
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Narrow::Short { len, held } => &held[..usize::from(*len)],
            Narrow::Long(bytes) => bytes,
        }
    }
}

impl Default for Narrow {
    fn default() -> Self {
        Narrow::new()
    }
}

impl std::ops::Deref for Narrow {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl PartialEq for Narrow {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Narrow {}

impl std::hash::Hash for Narrow {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl std::fmt::Debug for Narrow {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_slice().fmt(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{Hash, Hasher};

    fn hashed(text: &Narrow) -> u64 {
        let mut state = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut state);
        state.finish()
    }

    #[test]
    fn it_costs_the_space_a_vec_and_a_tag_already_did() {
        // The whole trade. If any of these grows, every string in every program
        // pays for the inline form whether it uses one or not — and the reason
        // to check rather than reason is that the reasoning said `Repr` would be
        // forty and it is thirty-two.
        assert_eq!(std::mem::size_of::<Narrow>(), 32);
        assert_eq!(
            std::mem::size_of::<super::super::Repr>(),
            32,
            "`Repr`'s discriminant goes in a niche of `Narrow`, so the wrapper is free"
        );
        assert_eq!(
            std::mem::size_of::<super::super::Str>(),
            40,
            "a string is what it was before the inline form existed"
        );
    }

    #[test]
    fn the_boundary_is_inline_on_one_side_and_spilled_on_the_other() {
        let fits = Narrow::from_slice(&[b'x'; INLINE]);
        let spills = Narrow::from_slice(&[b'x'; INLINE + 1]);
        assert!(matches!(fits, Narrow::Short { .. }));
        assert!(matches!(spills, Narrow::Long(_)));
        assert_eq!(fits.len(), INLINE);
        assert_eq!(spills.len(), INLINE + 1);
    }

    #[test]
    fn the_same_text_is_equal_and_hashes_alike_however_it_is_held() {
        // Built by hand: no constructor here spills a short string, and this is
        // what says it would not matter if one did.
        let inline = Narrow::from_slice(b"same");
        let spilled = Narrow::Long(b"same".to_vec());
        assert_eq!(inline, spilled);
        assert_eq!(hashed(&inline), hashed(&spilled));
        assert_ne!(inline, Narrow::from_slice(b"samf"));
    }

    #[test]
    fn every_constructor_agrees_about_the_bytes() {
        let units: Vec<u16> = b"caf\xe9 au lait".iter().map(|byte| u16::from(*byte)).collect();
        let expected: &[u8] = b"caf\xe9 au lait";
        assert_eq!(&*Narrow::from_units(&units), expected);
        assert_eq!(&*Narrow::from_vec(expected.to_vec()), expected);
        assert_eq!(&*Narrow::joined(b"caf\xe9 ", b"au lait"), expected);
        let long = [b'y'; 40];
        assert_eq!(&*Narrow::joined(&long[..25], &long[25..]), &long[..]);
        assert!(Narrow::new().is_empty());
    }
}
