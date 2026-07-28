//! PolyValue ↔ real handle bridge — the engine half of the new-engine's
//! NaN-boxed value model (`rts-codegen-new`).
//!
//! The new engine (`rts-codegen-new`) carries every polymorphic JS value as a
//! single 64-bit NaN-boxed word (`PolyValue`). A heap reference (string / object
//! / function) stores only the **48-bit payload** of the value (slot+shard), but
//! a real runtime handle is `gen(16) | slot(43) | shard(5)` and the engine's
//! `with_entry`/`free_handle`/`get` VALIDATE the 16-bit generation. The 48-bit
//! payload cannot carry that generation, so historically `rts-codegen-new` kept
//! a process-global indirection table (`Vec<u64>`) mapping a small idx → the full
//! real handle. That table was a wart AND a GC-root hole (it held the only strong
//! ref to those handles yet was not a GC root).
//!
//! This module removes the need for that table: the PolyValue payload now stores
//! the bare 48-bit slot+shard, and the generation is **reconstructed on demand**
//! from the slot's own *current* live generation
//! ([`super::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE`]). The GC marker
//! ([`super::handles::mark_handle`]) normalizes any NaN-boxed candidate word
//! through [`poly_handle_normalize`] before decoding, so NaN-boxed handle words
//! found on the stack / in `Entry::Vec`/`Map` children mark the underlying slot.
//!
//! ## Frozen NaN-box value-model ABI
//!
//! The constants below mirror `crates/rts-codegen-new/src/value/layout.rs`. They
//! are a FROZEN contract: do not change without changing both. Reconstruction is
//! benign even for a false positive — a real high-generation handle (gen in
//! `0xFFF8..=0xFFFF`) that happens to satisfy the box discriminator reconstructs
//! to *itself* because the live slot's generation is exactly that value.

use super::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE;

/// The negative-quiet-NaN base of the value model: sign=1, exponent all ones,
/// qNaN bit (51)=1 — i.e. the top 13 bits are all set. A word `w` is boxed iff
/// `(w & POLY_BOX_BASE) == POLY_BOX_BASE`.
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-codegen-new/src/value/layout.rs`; do not change without changing
/// both.
pub const POLY_BOX_BASE: u64 = 0xFFF8_0000_0000_0000;

/// Mask for the 48-bit payload (bits 47..=0) of a boxed word. Identical to the
/// engine's `HANDLE_SLOT_MASK` (low 48 bits) by construction.
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-codegen-new/src/value/layout.rs`; do not change without changing
/// both.
pub const POLY_PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Bit position of the 3-bit tag inside a boxed word (bits 50..=48).
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-codegen-new/src/value/layout.rs`; do not change without changing
/// both.
pub const POLY_TAG_SHIFT: u64 = 48;

/// Mask for the 3-bit tag once shifted down.
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-codegen-new/src/value/layout.rs`; do not change without changing
/// both.
pub const POLY_TAG_MASK: u64 = 0x7;

/// Tag for a string handle. `typeof` ⇒ `"string"`.
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-codegen-new/src/value/layout.rs`; do not change without changing
/// both.
pub const POLY_TAG_STR: u64 = 3;

/// Tag for an object/array/registered-class handle. `typeof` ⇒ `"object"`.
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-codegen-new/src/value/layout.rs`; do not change without changing
/// both.
pub const POLY_TAG_OBJECT: u64 = 4;

/// Tag for a function handle. `typeof` ⇒ `"function"`.
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-codegen-new/src/value/layout.rs`; do not change without changing
/// both.
pub const POLY_TAG_FUNCTION: u64 = 5;
/// Singleton tag (undefined/null/bool/hole/empty select via payload).
pub const POLY_TAG_SINGLETON: u64 = 2;

/// Tag for an inline 32-bit integer (payload = the `i32` bits, zero-extended).
/// `typeof` ⇒ `"number"`, like an inline `f64`.
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-runtime/src/adapters/value/layout.rs`; do not change without changing both.
pub const POLY_TAG_INT32: u64 = 1;

/// The `undefined` singleton word (`TAG_SINGLETON`, payload 0) — used to pad
/// unread callback-arg slots in the `__rtsadp_fn_invoke` bridge.
pub const POLY_UNDEFINED: u64 = POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT);

/// Payloads that select which singleton a `TAG_SINGLETON` word is.
///
/// Frozen NaN-box value-model ABI, mirrors
/// `crates/rts-runtime/src/adapters/value/layout.rs`; do not change without changing both.
pub const POLY_SING_UNDEFINED: u64 = 0;
pub const POLY_SING_NULL: u64 = 1;
pub const POLY_SING_FALSE: u64 = 2;
pub const POLY_SING_TRUE: u64 = 3;
pub const POLY_SING_HOLE: u64 = 4;
pub const POLY_SING_EMPTY: u64 = 5;

/// The `null` singleton word.
pub const POLY_NULL: u64 = POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT) | POLY_SING_NULL;
/// The `false` singleton word.
pub const POLY_FALSE: u64 =
    POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT) | POLY_SING_FALSE;
/// The `true` singleton word.
pub const POLY_TRUE: u64 = POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT) | POLY_SING_TRUE;

/// The boxed-word tag of `w`, or `None` if `w` is a genuine inline `f64`.
#[inline]
pub fn poly_tag(w: u64) -> Option<u64> {
    if (w & POLY_BOX_BASE) == POLY_BOX_BASE {
        Some((w >> POLY_TAG_SHIFT) & POLY_TAG_MASK)
    } else {
        None
    }
}

/// The `f64` value of a number word — an inline double or a boxed `INT32` —
/// or `None` if `w` is not a number.
#[inline]
pub fn poly_number(w: u64) -> Option<f64> {
    match poly_tag(w) {
        None => Some(f64::from_bits(w)),
        Some(POLY_TAG_INT32) => Some(((w & POLY_PAYLOAD_MASK) as u32) as i32 as f64),
        _ => None,
    }
}

/// If `c` is a NaN-boxed heap-handle word (boxed discriminator set AND tag ∈
/// {STR, OBJECT, FUNCTION}), reconstruct the full real runtime handle for its
/// 48-bit payload via [`super::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE`];
/// otherwise `None`.
///
/// A `None` result means "not a NaN-boxed handle" — either a plain `f64`/int/
/// singleton boxed word, or a raw real handle whose top 13 bits are NOT all set
/// (every real handle with generation < `0xFFF8` falls here). A high-generation
/// real handle (gen in `0xFFF8..=0xFFFF`) is a benign false positive: it does
/// satisfy the box discriminator, but `POLY_TO_HANDLE` reads the live slot's
/// generation — which is exactly that value — so it reconstructs to itself.
#[inline]
pub fn poly_handle_normalize(c: u64) -> Option<u64> {
    if (c & POLY_BOX_BASE) != POLY_BOX_BASE {
        return None;
    }
    let tag = (c >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
    if tag == POLY_TAG_STR || tag == POLY_TAG_OBJECT || tag == POLY_TAG_FUNCTION {
        Some(__RTS_FN_NS_GC_POLY_TO_HANDLE(c & POLY_PAYLOAD_MASK))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::handles::{
        __RTS_FN_NS_GC_POLY_FROM_HANDLE, Entry, HANDLE_SLOT_MASK, alloc_entry, free_handle,
    };

    const GEN_SHIFT: u64 = crate::abi::handles::HANDLE_GEN_SHIFT as u64;

    /// (a) FROM_HANDLE then TO_HANDLE round-trips to the original full handle for
    /// a live slot.
    #[test]
    fn from_then_to_handle_roundtrips_live_slot() {
        let h = alloc_entry(Entry::String(b"poly-roundtrip".to_vec()));
        let poly48 = __RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
        assert_eq!(
            poly48,
            h & HANDLE_SLOT_MASK,
            "FROM_HANDLE must drop the gen"
        );
        let reconstructed = __RTS_FN_NS_GC_POLY_TO_HANDLE(poly48);
        assert_eq!(reconstructed, h, "TO_HANDLE(FROM_HANDLE(h)) must equal h");
        free_handle(h);
    }

    /// (b) A real handle with generation < 0xFFF8 is NOT in the boxed space, so
    /// `normalize` returns None (it is a plain handle, not a NaN-boxed word).
    #[test]
    fn normalize_low_gen_real_handle_is_none() {
        let h = alloc_entry(Entry::String(b"low-gen".to_vec()));
        // A freshly-allocated slot has a small generation (1, or a small bump on
        // reuse) — far below 0xFFF8, so its top 13 bits are NOT all ones.
        let slot_gen = (h >> GEN_SHIFT) & 0xFFFF;
        assert!(
            slot_gen < 0xFFF8,
            "fresh handle gen {slot_gen:#x} unexpectedly high"
        );
        assert!(
            poly_handle_normalize(h).is_none(),
            "a low-generation real handle must not look NaN-boxed"
        );
        free_handle(h);
    }

    /// (c) A fabricated boxed-STR word over a live slot normalizes to that slot's
    /// live handle.
    #[test]
    fn normalize_boxed_str_over_live_slot() {
        let h = alloc_entry(Entry::String(b"boxed-str".to_vec()));
        let poly48 = __RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
        // Fabricate the boxed word exactly as codegen would: BOX_BASE | STR<<48 | payload.
        let boxed = POLY_BOX_BASE | (POLY_TAG_STR << POLY_TAG_SHIFT) | (poly48 & POLY_PAYLOAD_MASK);
        assert_eq!(
            poly_handle_normalize(boxed),
            Some(h),
            "boxed-STR over a live slot must reconstruct the live handle"
        );
        free_handle(h);
    }

    /// (d) THE COLLISION TEST. Fabricate a REAL handle whose generation lands in
    /// the `0xFFF8..=0xFFFF` range pointing at a LIVE slot, and assert
    /// `normalize`/`TO_HANDLE` reconstructs the SAME live handle (benign false
    /// positive) — proving a high-generation real handle is not corrupted.
    ///
    /// We pin allocation to ONE shard (`shards()[idx].alloc_in_shard`) and then
    /// force that live slot's generation to `0xFFF9` in place (test-only helper).
    /// Forcing avoids ~65k free/realloc cycles that would churn the global
    /// `LIVE_HANDLES` counter and race the counter-based tests; the generation
    /// value is exactly what real free/realloc would eventually reach.
    #[test]
    fn collision_high_gen_real_handle_reconstructs_self() {
        use crate::heap::handles::{decode, shards};

        let shard_idx = 7usize;
        let h0 = shards()[shard_idx]
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .alloc_in_shard(Entry::String(b"collision".to_vec()), shard_idx);
        let (_, _, table_slot) = decode(h0).expect("freshly allocated handle decodes");

        // Force the slot into a high generation (0xFFFB ∈ 0xFFF8..=0xFFFF). The
        // re-encoded handle's top 13 bits (63..=51) are all set, so it LOOKS
        // NaN-boxed; its tag bits (50..=48) = low 3 bits of the gen = 0b011 = 3 =
        // TAG_STR, so it even parses as a boxed *string* word — the worst-case
        // false positive, which must still reconstruct to itself.
        let high_gen: u16 = 0xFFFB;
        let hg = shards()[shard_idx]
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .force_slot_generation(shard_idx, table_slot as usize, high_gen)
            .expect("force_slot_generation on a live slot");
        assert_eq!(
            (hg >> GEN_SHIFT) & 0xFFFF,
            high_gen as u64,
            "forced handle must carry the high generation"
        );
        // Regardless of the box check, TO_HANDLE reads the live slot's generation,
        // which IS this handle's generation, so it reconstructs to itself.
        let payload = hg & HANDLE_SLOT_MASK;
        assert_eq!(
            __RTS_FN_NS_GC_POLY_TO_HANDLE(payload),
            hg,
            "high-gen handle must reconstruct to itself (no corruption)"
        );
        // If it also parses as a boxed handle word, normalize must agree.
        if (hg & POLY_BOX_BASE) == POLY_BOX_BASE {
            let tag = (hg >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
            if tag == POLY_TAG_STR || tag == POLY_TAG_OBJECT || tag == POLY_TAG_FUNCTION {
                assert_eq!(
                    poly_handle_normalize(hg),
                    Some(hg),
                    "high-gen boxed-looking handle must normalize to itself"
                );
            }
        }
        free_handle(hg);
    }

    /// TO_HANDLE returns 0 for an out-of-range slot index (no live slot).
    #[test]
    fn to_handle_out_of_range_is_zero() {
        let bogus_payload = 0x0000_7FFF_FFFF_FFFFu64 & POLY_PAYLOAD_MASK;
        assert_eq!(__RTS_FN_NS_GC_POLY_TO_HANDLE(bogus_payload), 0);
    }

    /// Regression: a handle whose payload is exactly 0 (slot 0 of shard 0 — the
    /// FIRST allocation in that shard) MUST round-trip. An early "payload == 0 ⇒
    /// invalid" short-circuit silently turned that legitimate handle into a dead
    /// reference (the space-separator string in the new engine's console.log
    /// landed there). TO_HANDLE(0) must reconstruct the live handle, not 0.
    #[test]
    fn payload_zero_slot_is_valid_not_sentinel() {
        use crate::heap::handles::{decode, shards};

        // Force-occupy slot 0 of shard 0 so its payload is exactly 0.
        let h = shards()[0]
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .alloc_in_shard(Entry::String(b"slot-zero".to_vec()), 0);
        let (_, _, table_slot) = decode(h).expect("decodes");
        // Only meaningful when this really is slot 0 / shard 0 (payload 0).
        let payload = __RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
        if table_slot == 0 {
            assert_eq!(payload, 0, "slot 0 of shard 0 must have payload 0");
            assert_eq!(
                __RTS_FN_NS_GC_POLY_TO_HANDLE(0),
                h,
                "payload 0 must reconstruct the live slot-0 handle, not the sentinel"
            );
        }
        free_handle(h);
    }
}
