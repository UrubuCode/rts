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
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_get_by_payload(poly48: u64, index: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    with_payload_slot(poly48, |entry| match entry {
        Some(Entry::Vec(v)) => v.get(index as usize).copied().unwrap_or(0),
        _ => 0,
    })
}

/// Store `value` at `index`, growing with zeros the way the unfused path does.
/// Returns 1 on success, 0 when the slot is invalid or not a vec, matching the
/// existing `VEC_SET` contract.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_set_by_payload(poly48: u64, index: i64, value: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    with_payload_slot_mut(poly48, |entry| match entry {
        Some(Entry::Vec(v)) => {
            let i = index as usize;
            if i >= v.len() {
                v.resize(i + 1, 0);
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
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_push_by_payload(poly48: u64, value: i64) -> i64 {
    with_payload_slot_mut(poly48, |entry| match entry {
        Some(Entry::Vec(v)) => {
            v.push(value);
            1
        }
        _ => 0,
    })
}

/// Length of the vec at `poly48`, or -1 for an invalid slot / non-vec.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_len_by_payload(poly48: u64) -> i64 {
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
    fn set_beyond_len_grows_with_zeros() {
        let h = alloc_entry(Entry::Vec(Box::new(vec![1])));
        let payload = payload_of(h);
        assert_eq!(__rtsn_vec_set_by_payload(payload, 3, 9), 1);
        assert_eq!(__rtsn_vec_get_by_payload(payload, 2), 0);
        assert_eq!(__rtsn_vec_get_by_payload(payload, 3), 9);
        assert_eq!(__rtsn_vec_len_by_payload(payload), 4);
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
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_vec_new_object() -> u64 {
    let handle = alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let payload = handle & SLOT_MASK;
    crate::heap::poly::POLY_BOX_BASE
        | (crate::heap::poly::POLY_TAG_OBJECT << crate::heap::poly::POLY_TAG_SHIFT)
        | payload
}
