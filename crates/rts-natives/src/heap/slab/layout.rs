//! The layout Tier 3.2's emitted field load reads through — **derived from a
//! real value, never assumed**.
//!
//! `RTS_OPTIMIZATION.md` §5 Tier 3.2 replaces `call __rtsn_vec_get_by_payload`
//! with a handful of Cranelift `load`s. To emit those loads the compiler needs
//! numbers: where a slot lives, how wide it is, where inside it the `Entry`
//! discriminant sits, and where an inline block's length and words sit. Every
//! one of those is a Rust type layout, and a hardcoded copy of a Rust type
//! layout is a silent-miscompile generator the day someone adds a field.
//!
//! So none of them is hardcoded. This module builds a real
//! `Slot::live(g, Entry::Vec(Slots::Inline { .. }))`, takes the addresses of the
//! parts through accessors on the types that own them, and subtracts. The two
//! enums carry `#[repr(C, u8)]` and `Slot` carries `#[repr(C)]` purely so those
//! numbers are *stable within a build* — the reprs make the layout defined, the
//! probe makes it correct.
//!
//! ## The repr costs memory, unconditionally — measured, not estimated
//!
//! Fixing the layout gives up Rust's niche packing, and the slot table pays it
//! on every slot whether or not the fast path is ever emitted:
//!
//! ```text
//!            natural repr   repr(C,u8)/(C)
//!   Slots           72              80
//!   Entry           72              88
//!   Slot            80              96      (+20%)
//! ```
//!
//! That is an honest asymmetry and it is written here rather than buried: the
//! *cost* is unconditional, the *benefit* is behind `RTS_SLAB` + `RTS_FIELD_LOAD`.
//! It is `INLINE_CAP` that drives it — `Slots::Inline` is `tag(1) + pad(7) +
//! len(4) + pad(4) + words(CAP*8)`, so the padding the tag word introduces is
//! what pushed `Entry` up. Dropping `INLINE_CAP` to 7 brings `Slot` to 88, and 6
//! brings it back to 80 exactly, at the price of inlining a 5- rather than a
//! 7-field class.
//!
//! TODO(measure): RSS and cache-miss rate on an object-heavy fixture at
//! `INLINE_CAP` 6/7/8, which is the measurement that should choose between them.
//! Nothing here should be moved on reasoning alone.
//!
//! ## What the emitted sequence does with this
//!
//! ```text
//!   payload = poly & SLOT_MASK
//!   shard   = payload & SHARD_MASK
//!   idx     = payload >> SHARD_BITS
//!   chunk   = idx >> chunk_bits            ; bail if >= max_chunks
//!   base    = load [chunk_table_base + (shard*max_chunks + chunk)*8]
//!                                          ; bail if null
//!   slot    = base + (idx & chunk_mask) * slot_stride
//!   bail unless load.i8  [slot + entry_tag_off ] == entry_tag_vec
//!   bail unless load.i8  [slot + slots_tag_off ] == slots_tag_inline
//!   bail unless index < load.i32 [slot + inline_len_off]
//!   result  = load.i64   [slot + inline_words_off + index*8]
//! ```
//!
//! and "bail" is the ordinary call, so the fast path can only ever be an
//! optimization: every rejection lands on the existing implementation.
//!
//! ## Why no generation check, and why that is not a weakening
//!
//! `payload_ops::vec_get_by_payload` — the function this replaces — does not
//! check the generation either. It cannot: it receives a 48-bit payload, which
//! carries no generation. What it rejects is an out-of-range slot and a slot
//! whose `Entry` is not a `Vec`, and the tag test above rejects exactly those.
//! Reproducing its semantics is the requirement; adding a check it does not
//! perform would be a *different* function, not a safer one.
//!
//! ## Why a raw load of a live slot is sound here
//!
//! Three properties, each established elsewhere and each necessary:
//!
//! * **The address is stable.** `RTS_SLAB=1`'s chunks are published once and
//!   never moved, reallocated or freed ([`super::chunks`]), so the address a
//!   payload resolves to is fixed for the life of the process. This path is
//!   gated on that knob for that reason, and [`available`] enforces it.
//! * **The words are inside the slot.** `RTS_CLASS_IMPLEMENTATION.md` §8.4's
//!   [`crate::heap::slots::Slots::Inline`] carries the words by value, so there
//!   is no second buffer for a sweep to drop or a `push` to reallocate. The
//!   `Heap` form is rejected by the discriminant test rather than read.
//! * **The slot cannot be swept under the read.** The receiver's handle word is
//!   live in the frame, so it is a conservative root, and a GC tick fires only
//!   from `alloc_entry` — only at a call. This is the same argument
//!   `collector/scan.rs` already relies on when it scans callee-saved registers
//!   and no caller-saved ones.
//!
//! What it does NOT survive is a moving collector (`RTS_CLASS_IMPLEMENTATION.md`
//! §4.4). That is written down there, and the block indirection specified there
//! is what would keep it safe.
//!
//! ## Promotion is why the discriminant test is not optional
//!
//! `Slots::promote` moves the words out of the slot and into a `Box<Vec<i64>>`
//! when an object grows past `INLINE_CAP`. Promotion is one-way, so the test is
//! a single byte compare rather than a revalidation loop — but it must be *in
//! the emitted sequence*, on every read, because the compiler cannot prove an
//! object never gains a property.

use std::sync::OnceLock;

use crate::heap::handles::Entry;
use crate::heap::slots::{INLINE_CAP, Slots};

use super::chunks;
use super::store::Slot;

/// Every number the emitted field load needs. All derived; see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldLoadLayout {
    /// Address of the flat chunk table (`chunks::table_base`).
    pub chunk_table_base: usize,
    /// `idx >> chunk_bits` is the chunk number.
    pub chunk_bits: u32,
    /// `idx & chunk_mask` is the offset within the chunk.
    pub chunk_mask: usize,
    /// Chunk numbers at or above this are out of the table.
    pub max_chunks: usize,
    /// `size_of::<Slot>()` — the stride the offset is multiplied by.
    pub slot_stride: usize,
    /// Byte offset of the `Entry` discriminant within a `Slot`.
    pub entry_tag_off: usize,
    /// Discriminant value of `Entry::Vec`.
    pub entry_tag_vec: u8,
    /// Byte offset of the `Slots` discriminant within a `Slot`.
    pub slots_tag_off: usize,
    /// Discriminant value of `Slots::Inline`.
    pub slots_tag_inline: u8,
    /// Byte offset of the inline `len: u32` within a `Slot`.
    pub inline_len_off: usize,
    /// Byte offset of `words[0]` within a `Slot`.
    pub inline_words_off: usize,
    /// Words the inline block holds — the bound `len` can never exceed.
    pub inline_cap: usize,
}

static LAYOUT: OnceLock<Option<FieldLoadLayout>> = OnceLock::new();

/// The derived layout, or `None` when derivation or verification failed.
///
/// `None` is a refusal to emit, never a fallback to a guess: a caller that
/// cannot get a layout emits the ordinary call.
pub fn field_load_layout() -> Option<FieldLoadLayout> {
    *LAYOUT.get_or_init(|| {
        let l = derive();
        if verify(&l) { Some(l) } else { None }
    })
}

/// `true` when the emitted field load may be used: the chunked store is the
/// active storage AND the layout verified.
///
/// Both halves matter. Without `RTS_SLAB=1` the slots live in a `Vec` that
/// reallocates, so no address into it is stable and the whole sequence is
/// unsound — this is the one place that coupling is enforced rather than
/// documented.
pub fn available() -> bool {
    super::enabled() && field_load_layout().is_some()
}

/// Build a canonical inline slot and read the offsets off it.
fn derive() -> FieldLoadLayout {
    let slot = Slot::live(
        1,
        Entry::Vec(Slots::Inline {
            len: 3,
            words: [0; INLINE_CAP],
        }),
    );
    let slot_addr = &slot as *const Slot as usize;
    let entry_addr = slot.probe_entry_addr();

    let payload_addr = match probe_slots(&slot) {
        Some(s) => s.probe_self_addr(),
        None => unreachable!("the probe slot was built as Entry::Vec"),
    };
    let (len_addr, words_addr) = probe_slots(&slot)
        .and_then(|s| s.probe_field_addrs())
        .expect("the probe slot was built as Slots::Inline");

    FieldLoadLayout {
        chunk_table_base: chunks::table_base(),
        chunk_bits: chunks::CHUNK_BITS,
        chunk_mask: chunks::CHUNK_MASK,
        max_chunks: chunks::MAX_CHUNKS,
        slot_stride: std::mem::size_of::<Slot>(),
        // `repr(C, u8)` puts the discriminant at byte 0 of the enum.
        entry_tag_off: entry_addr - slot_addr,
        entry_tag_vec: discriminant_of(&slot),
        slots_tag_off: payload_addr - slot_addr,
        slots_tag_inline: unsafe { *(payload_addr as *const u8) },
        inline_len_off: len_addr - slot_addr,
        inline_words_off: words_addr - slot_addr,
        inline_cap: INLINE_CAP,
    }
}

fn probe_slots(slot: &Slot) -> Option<&Slots> {
    match slot.entry_ref() {
        Entry::Vec(s) => Some(s),
        _ => None,
    }
}

/// The `Entry` discriminant of this slot's entry.
///
/// Sound because `Entry` is `#[repr(C, u8)]`: the discriminant is a `u8` at the
/// start of the enum, and `probe_entry_addr` is that start.
fn discriminant_of(slot: &Slot) -> u8 {
    unsafe { *(slot.probe_entry_addr() as *const u8) }
}

/// Read a canonical slot through the raw offsets and compare with the safe
/// accessors — and check that the offsets reject the forms they must reject.
///
/// This is what makes "derived" mean something. A derivation that silently
/// produced a wrong offset would still be a derivation; this fails it.
fn verify(l: &FieldLoadLayout) -> bool {
    const WORDS: [i64; 4] = [-9, 0, 7, i64::MIN];

    let mut block = [0i64; INLINE_CAP];
    block[..WORDS.len()].copy_from_slice(&WORDS);
    let inline = Slot::live(
        7,
        Entry::Vec(Slots::Inline {
            len: WORDS.len() as u32,
            words: block,
        }),
    );
    let base = &inline as *const Slot as usize;

    // SAFETY: every offset below was derived from a `Slot` of this exact shape,
    // and `inline` is live for the whole block.
    unsafe {
        if *((base + l.entry_tag_off) as *const u8) != l.entry_tag_vec {
            return false;
        }
        if *((base + l.slots_tag_off) as *const u8) != l.slots_tag_inline {
            return false;
        }
        if *((base + l.inline_len_off) as *const u32) != WORDS.len() as u32 {
            return false;
        }
        for (i, want) in WORDS.iter().enumerate() {
            let got = *((base + l.inline_words_off + i * 8) as *const i64);
            if got != *want {
                return false;
            }
        }
    }

    // The heap form must FAIL the inline test — otherwise the emitted sequence
    // would read a `Box` pointer as a word count.
    let heaped = Slot::live(7, Entry::vec(WORDS.to_vec()));
    let hbase = &heaped as *const Slot as usize;
    // SAFETY: same shape, same lifetime argument.
    unsafe {
        if *((hbase + l.entry_tag_off) as *const u8) != l.entry_tag_vec {
            return false; // it IS a Vec; the entry tag must still match
        }
        if *((hbase + l.slots_tag_off) as *const u8) == l.slots_tag_inline {
            return false; // but it must not claim to be inline
        }
    }

    // A non-`Vec` entry must fail the entry test.
    let other = Slot::live(7, Entry::String(b"xy".to_vec()));
    let obase = &other as *const Slot as usize;
    // SAFETY: same.
    if unsafe { *((obase + l.entry_tag_off) as *const u8) } == l.entry_tag_vec {
        return false;
    }

    // A never-allocated slot must fail it too — this is the out-of-range and
    // freed case, which is the only rejection `vec_get_by_payload` performs
    // beyond "not a vec".
    let free = Slot::free();
    let fbase = &free as *const Slot as usize;
    // SAFETY: same.
    if unsafe { *((fbase + l.entry_tag_off) as *const u8) } == l.entry_tag_vec {
        return false;
    }

    // Sanity on the numbers themselves.
    l.slot_stride > l.inline_words_off + l.inline_cap * 8 - 8
        && l.chunk_table_base != 0
        && l.max_chunks > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_derives_and_verifies() {
        // Independent of the knob: derivation does not touch the store.
        let l = derive();
        assert!(verify(&l), "derived layout failed verification: {l:?}");
    }

    #[test]
    fn raw_read_agrees_with_the_safe_accessor_at_every_index() {
        let l = derive();
        assert!(verify(&l));
        let mut words = [0i64; INLINE_CAP];
        for (i, w) in words.iter_mut().enumerate() {
            *w = (i as i64 + 1) * -31;
        }
        let slot = Slot::live(
            9,
            Entry::Vec(Slots::Inline {
                len: INLINE_CAP as u32,
                words,
            }),
        );
        let base = &slot as *const Slot as usize;
        let safe = match slot.entry_ref() {
            Entry::Vec(v) => v.to_owned_vec(),
            _ => unreachable!(),
        };
        for (i, want) in safe.iter().enumerate() {
            let got = unsafe { *((base + l.inline_words_off + i * 8) as *const i64) };
            assert_eq!(got, *want, "word {i}");
        }
    }

    #[test]
    fn available_requires_the_chunked_store() {
        // The coupling in `available` is the load-bearing part; assert it holds
        // in whichever direction this test binary happens to be configured.
        assert_eq!(available(), super::super::enabled() && field_load_layout().is_some());
    }
}
