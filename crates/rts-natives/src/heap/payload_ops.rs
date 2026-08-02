//! Fused payload-addressed slot access — one shard lock per field touch.
//!
//! ## The redundancy this removes
//!
//! A field read lowers to TWO extern calls that each take the shard lock:
//!
//! 1. `POLY_TO_HANDLE(payload)` locks the shard, READS `slot.generation`, and
//!    returns `gen << 48 | payload`.
//! 2. `VEC_GET(handle, i)` locks the same shard again and calls
//!    `HandleTable::get`, which decodes the generation back out of the handle and
//!    compares it to `slot.generation`.
//!
//! The comparison in (2) is against the value read in (1), from the same slot, so
//! it can only ever succeed. The pair does not detect a stale payload — it cannot,
//! because step 1 reconstructs the handle from whatever generation is live NOW.
//! What it does detect is a freed or out-of-range slot, and that is preserved
//! exactly below.
//!
//! So the generation round-trip is ceremony: two calls, two lock/unlock pairs and
//! two optimization barriers, to validate a value against itself. These entry
//! points take the 48-bit payload directly and do the work under ONE lock.
//!
//! Measured on `bench/objbench_noalloc.ts` (3M field reads, no allocation in the
//! loop), the pre-existing in-loop call profile was 2x `POLY_TO_HANDLE`,
//! 2x `VEC_GET`, plus the generic add/mul — so collapsing the pair removes a
//! third of the calls and half the lock traffic on the read path.
//!
//! ## Why this is not a `NativeEmit`
//!
//! An earlier reading called `POLY_TO_HANDLE` "pure bit twiddling" and proposed
//! emitting it as inline IR. That was wrong: it READS the generation out of the
//! live slot, which is heap traversal, and the `NativeEmit` contract in
//! `CLAUDE.md` scopes emitters to scalar computation precisely because the
//! HandleTable and the allocator do not exist at the IR level. Fusing the two
//! calls is the version of that idea which survives the constraint.

use crate::heap::handles::{Entry, alloc_entry, shards};

use crate::abi::handles::{
    HANDLE_SHARD_BITS as SHARD_BITS, HANDLE_SHARD_MASK as SHARD_MASK,
    HANDLE_SLOT_MASK as SLOT_MASK,
};

/// Run `f` against the slot named by a raw 48-bit PolyValue payload, under one
/// shard lock. `None` is passed for an out-of-range or freed slot — the same two
/// conditions the `POLY_TO_HANDLE` + `get` pair rejects today.
///
/// NB: a payload of 0 is the perfectly valid (slot 0, shard 0) live slot — the
/// first handle allocated in shard 0 — so there is deliberately no zero
/// short-circuit here. Treating payload 0 as invalid silently broke every value
/// that landed in that slot once before.
#[inline]
fn with_payload_slot<R>(poly48: u64, f: impl FnOnce(Option<&Entry>) -> R) -> R {
    let payload = poly48 & SLOT_MASK;
    let shard_idx = (payload & SHARD_MASK) as usize;
    let table_slot = (payload >> SHARD_BITS) as usize;
    let guard = shards()[shard_idx]
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    f(guard.entry_at(table_slot))
}

/// Mutable sibling of [`with_payload_slot`].
#[inline]
fn with_payload_slot_mut<R>(poly48: u64, f: impl FnOnce(Option<&mut Entry>) -> R) -> R {
    let payload = poly48 & SLOT_MASK;
    let shard_idx = (payload & SHARD_MASK) as usize;
    let table_slot = (payload >> SHARD_BITS) as usize;
    let mut guard = shards()[shard_idx]
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    f(guard.entry_at_mut(table_slot))
}

/// Element `index` of the vec at `poly48`, or 0 when the slot is invalid, is not
/// a vec, or the index is out of range — byte-for-byte the result the
/// `POLY_TO_HANDLE` + `VEC_GET` pair produces today.
#[rtse::abi(native, value = "vec_get_by_payload")]
pub fn vec_get_by_payload(poly48: u64, index: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    with_payload_slot(poly48, |entry| match entry {
        Some(Entry::Vec(v)) => v.get(index as usize).copied().unwrap_or(0),
        _ => 0,
    })
}

/// The array-HOLE singleton as a raw `PolyValue` word. Writing past the end of a
/// JS array creates HOLES, not zeros: `a = [1,2,3]; a[6] = 7` must leave indices
/// 3..5 absent, so `a[4]` is `undefined`, `4 in a` is `false`, and
/// `JSON.stringify` renders `null`. Growing with `0` made every skipped index a
/// real element holding the number zero.
///
/// This is the same word `PolyValue::hole()` builds and the same one the read
/// side (`emit_hole_to_undef`, and the `is_hole` checks across the array
/// callbacks) tests for — the legacy `i64::MIN + 4` sentinel used by the old
/// engine's `VEC_SET_LENGTH` is a DIFFERENT scheme and is not what this path's
/// readers look at.
const HOLE_WORD: i64 = (crate::heap::poly::POLY_BOX_BASE
    | (crate::heap::poly::POLY_TAG_SINGLETON << crate::heap::poly::POLY_TAG_SHIFT)
    | crate::heap::poly::POLY_SING_HOLE) as i64;

/// Store `value` at `index`, growing with HOLES (JS sparse-array semantics; see
/// [`HOLE_WORD`]). Returns 1 on success, 0 when the slot is invalid or not a vec,
/// matching the existing `VEC_SET` contract.
#[rtse::abi(native, value = "vec_set_by_payload")]
pub fn vec_set_by_payload(poly48: u64, index: i64, value: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    with_payload_slot_mut(poly48, |entry| match entry {
        Some(Entry::Vec(v)) => {
            let i = index as usize;
            if i >= v.len() {
                v.resize(i + 1, HOLE_WORD);
            }
            v[i] = value;
            1
        }
        _ => 0,
    })
}

/// Append `value`, returning 1 on success and 0 for an invalid slot / non-vec.
///
/// The growth happens INSIDE the lock scope, exactly as the fused setter's
/// `resize` does, so a reallocation of the backing buffer cannot be observed by
/// anything holding a stale pointer — no pointer survives the call boundary in
/// either direction.
#[rtse::abi(native, value = "vec_push_by_payload")]
pub fn vec_push_by_payload(poly48: u64, value: i64) -> i64 {
    with_payload_slot_mut(poly48, |entry| match entry {
        Some(Entry::Vec(v)) => {
            v.push(value);
            1
        }
        _ => 0,
    })
}

/// Length of the vec at `poly48`, or -1 for an invalid slot / non-vec.
#[rtse::abi(native, value = "vec_len_by_payload")]
pub fn vec_len_by_payload(poly48: u64) -> i64 {
    with_payload_slot(poly48, |entry| match entry {
        Some(Entry::Vec(v)) => v.len() as i64,
        _ => -1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::handles::{Entry, alloc_entry};

    /// The 48-bit payload a PolyValue carries is exactly the handle with its
    /// generation stripped.
    fn payload_of(handle: u64) -> u64 {
        handle & SLOT_MASK
    }

    #[test]
    fn fused_get_matches_the_unfused_pair() {
        let h = alloc_entry(Entry::Vec(Box::new(vec![10, 20, 30])));
        let payload = payload_of(h);
        assert_eq!(__rtsn_vec_get_by_payload(payload, 0), 10);
        assert_eq!(__rtsn_vec_get_by_payload(payload, 2), 30);
        // Out of range and negative both yield 0, as the unfused pair does.
        assert_eq!(__rtsn_vec_get_by_payload(payload, 9), 0);
        assert_eq!(__rtsn_vec_get_by_payload(payload, -1), 0);
    }

    #[test]
    fn fused_set_then_get_round_trips() {
        let h = alloc_entry(Entry::Vec(Box::new(vec![0, 0])));
        let payload = payload_of(h);
        assert_eq!(__rtsn_vec_set_by_payload(payload, 1, 77), 1);
        assert_eq!(__rtsn_vec_get_by_payload(payload, 1), 77);
        assert_eq!(__rtsn_vec_len_by_payload(payload), 2);
    }

    #[test]
    /// Growing past the end creates JS array HOLES, not zeros.
    ///
    /// This test previously asserted the skipped slots read back as `0`, which is
    /// what the implementation did and what made `a = [1,2,3]; a[6] = 7` leave
    /// real zero elements behind: `a[4]` was `0` instead of `undefined`, `4 in a`
    /// was `true`, and `JSON.stringify` printed `0` where every other runtime
    /// prints `null`. The expectation was wrong, not the observation — updated
    /// deliberately along with the fix.
    fn set_beyond_len_grows_with_holes() {
        let h = alloc_entry(Entry::Vec(Box::new(vec![1])));
        let payload = payload_of(h);
        assert_eq!(__rtsn_vec_set_by_payload(payload, 3, 9), 1);
        assert_eq!(
            __rtsn_vec_get_by_payload(payload, 2),
            HOLE_WORD,
            "a skipped index must be a HOLE, not a real zero element"
        );
        assert_eq!(__rtsn_vec_get_by_payload(payload, 3), 9);
        assert_eq!(__rtsn_vec_len_by_payload(payload), 4);
    }

    #[test]
    /// The fused instance allocation must be slot-for-slot what the
    /// `vec_new_object` + N x `vec_push_by_payload` sequence produced.
    fn shaped_alloc_matches_the_unfused_sequence() {
        let shape_word = 0x1234_i64;

        let unfused = __rtsn_vec_new_object();
        let up = payload_of(unfused);
        __rtsn_vec_push_by_payload(up, shape_word);
        __rtsn_vec_push_by_payload(up, crate::heap::poly::POLY_UNDEFINED as i64);
        __rtsn_vec_push_by_payload(up, crate::heap::poly::POLY_UNDEFINED as i64);

        let fused = __rtsn_vec_new_object_shaped(shape_word, 2);
        let fp = payload_of(fused);

        // Same tag/header bits (only the payload differs — two distinct slots).
        assert_eq!(unfused & !SLOT_MASK, fused & !SLOT_MASK);
        assert_eq!(__rtsn_vec_len_by_payload(fp), 3);
        for i in 0..3 {
            assert_eq!(
                __rtsn_vec_get_by_payload(fp, i),
                __rtsn_vec_get_by_payload(up, i),
                "slot {i} differs from the unfused sequence"
            );
        }
    }

    #[test]
    /// A zero-field class is still `[shape_id]` — one slot, never empty.
    fn shaped_alloc_with_no_fields_keeps_slot_zero() {
        let w = __rtsn_vec_new_object_shaped(7, 0);
        let p = payload_of(w);
        assert_eq!(__rtsn_vec_len_by_payload(p), 1);
        assert_eq!(__rtsn_vec_get_by_payload(p, 0), 7);
    }

    #[test]
    fn non_vec_entry_is_rejected_not_misread() {
        let h = alloc_entry(Entry::String(b"not a vec".to_vec()));
        let payload = payload_of(h);
        assert_eq!(__rtsn_vec_get_by_payload(payload, 0), 0);
        assert_eq!(__rtsn_vec_len_by_payload(payload), -1);
    }
}

/// Allocate a fresh empty `Entry::Vec` and return it ALREADY BOXED as a
/// `TAG_OBJECT` PolyValue word.
///
/// The unfused spelling was two extern calls: `VEC_NEW` returning a real
/// handle, then `POLY_FROM_HANDLE` stripping the generation back off to get the
/// 48-bit payload the box needs. Nothing between them can observe the handle —
/// the caller wanted the boxed word all along — so the round trip was pure
/// ceremony, and it sat on the object-construction path at 25 emit sites.
///
/// Same argument as the `by_payload` accessors above: the generation is read and
/// then immediately discarded by the very next call.
#[rtse::abi(native, value = "vec_new_object")]
pub fn vec_new_object() -> u64 {
    // Smallest pooled class (Tier 4.2): an object built key-by-key still ends up
    // in the same size range as a shaped one, and asking for capacity here costs
    // nothing the first `push` would not have paid anyway.
    let handle = alloc_entry(Entry::Vec(crate::heap::bump::acquire(4)));
    let payload = handle & SLOT_MASK;
    crate::heap::poly::POLY_BOX_BASE
        | (crate::heap::poly::POLY_TAG_OBJECT << crate::heap::poly::POLY_TAG_SHIFT)
        | payload
}

/// Allocate an instance `Entry::Vec` ALREADY LAID OUT — slot 0 = `shape_word`,
/// slots `1..=field_count` = `undefined` — and return it boxed as a `TAG_OBJECT`
/// PolyValue word. The single-call spelling of what `new C(...)` used to emit as
/// `1 + 1 + N` calls.
///
/// ## Why this exists
///
/// `new P(x, y)` on a 2-field class emitted: `vec_new_object` (alloc, one shard
/// lock), `vec_push_by_payload` for the shape id (a second lock), then one
/// `vec_push_by_payload` per field writing the `undefined` placeholder (N more
/// locks) — and the constructor body then OVERWRITES every one of those
/// placeholders with `vec_set_by_payload` microseconds later. So an N-field class
/// paid `2 + N` locked extern calls before the ctor ran, N of them provably dead
/// stores. Here the whole layout is built in the `Vec` BEFORE `alloc_entry`
/// publishes it, so the cost is the one lock `alloc_entry` takes anyway.
///
/// The placeholders are still written (not skipped): the slots must EXIST with a
/// defined value, because a ctor that leaves a field unassigned, an early
/// `throw`, or a read of `this.f` before its assignment must all observe
/// `undefined` rather than a short vec whose `vec_get` returns 0. Same slot
/// count, same slot values, same handle semantics as the unfused sequence — this
/// is a call-count reduction, not a layout change.
///
/// `shape_word` is passed in rather than rebuilt here so the tagged-int encoding
/// of a shape id has exactly ONE definition, the codegen's
/// `PolyValue::from_i32(id).raw()`. `field_count` is a plain count; a negative
/// value is clamped to 0 (the ABI carries it as `i64`, and a bogus count must not
/// become a giant allocation).
#[rtse::abi(native, value = "vec_new_object_shaped")]
pub fn vec_new_object_shaped(shape_word: i64, field_count: i64) -> u64 {
    let n = field_count.max(0) as usize;
    // The payload buffer comes from this thread's recycler (Tier 4.2,
    // `heap::bump`) rather than from a fresh `malloc`. With `RTS_BUMP` unset
    // `acquire` IS `Box::new(Vec::with_capacity(1 + n))`.
    let mut slots = crate::heap::bump::acquire(1 + n);
    slots.push(shape_word);
    // The `undefined` word comes from the shared PolyValue constants, not a
    // literal bit pattern — the codegen bakes the same word via
    // `PolyValue::undefined().raw()`, and the two must never be able to drift.
    slots.resize(1 + n, crate::heap::poly::POLY_UNDEFINED as i64);
    let handle = alloc_entry(Entry::Vec(slots));
    let payload = handle & SLOT_MASK;
    crate::heap::poly::POLY_BOX_BASE
        | (crate::heap::poly::POLY_TAG_OBJECT << crate::heap::poly::POLY_TAG_SHIFT)
        | payload
}
