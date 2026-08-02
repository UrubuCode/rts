//! The flat chunk table — the one load that turns `(shard, chunk)` into a base
//! address.
//!
//! See [`super`] for the design, the ABA argument and the GC story. This file
//! owns only the table itself: its shape, its publication protocol and its
//! bounds.

use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use crate::abi::handles::HANDLE_N_SHARDS as N_SHARDS;

use super::store::Slot;

/// Slots per chunk. Power of two so `idx -> (chunk, off)` is a shift and a mask
/// rather than a division.
///
/// 512 is a compromise between two costs nobody has measured yet, so it is a
/// starting point and not a result: a *larger* chunk wastes memory on a shard
/// that allocates a handful of slots (a chunk is committed whole, and there are
/// 32 shards, so the floor is paid 32× on a program that touches every shard),
/// while a *smaller* chunk means more chunk-table entries and a shorter run of
/// address-adjacent slots for the sweep to walk. TODO(measure).
pub const CHUNK_BITS: u32 = 9;
pub const CHUNK_SLOTS: usize = 1 << CHUNK_BITS;
pub const CHUNK_MASK: usize = CHUNK_SLOTS - 1;

/// Chunks reserved per shard. Fixed, because the table must never move: growing
/// it would reallocate exactly the structure whose stability is the point.
///
/// `MAX_CHUNKS * CHUNK_SLOTS` = 2 097 152 slots per shard, 67 108 864 across the
/// 32 shards. The runtime already aborts at `HANDLES_MAX` (5 000 000 live
/// handles, `crate::heap::live`), and a shard would have to hold 2 M slots by
/// itself to exhaust this — over 13× the whole-process abort cap even if every
/// live handle landed in one shard. **Exhaustion is therefore unreachable behind
/// an earlier, better error**, which is why the exhaustion path below panics
/// with a diagnostic instead of trying to grow.
pub const MAX_CHUNKS: usize = 4096;

/// Entries in the flat table: `[shard * MAX_CHUNKS + chunk] -> base`.
const TABLE_LEN: usize = N_SHARDS * MAX_CHUNKS;

/// The table. A `static` array of atomic pointers, zero-initialized in BSS.
///
/// Three properties are load-bearing, and all three come from it being a fixed
/// `static` rather than a `Vec` or a `OnceLock`:
///
/// * **Its address never changes**, so [`table_base`] is safe to bake into
///   generated code as an immediate.
/// * **It needs no initialization**, so the allocator's growth path cannot
///   re-enter a lazy initializer while holding a shard `Mutex`.
/// * **It is flat**, so `(shard, chunk)` is one index computation and ONE load —
///   the property kernel H's H7 row measured as making chunking free.
///
/// Cost: `32 * 4096 * 8` = 1 MiB of demand-zero BSS, touched only where chunks
/// are actually published.
static CHUNK_TABLE: [AtomicPtr<Slot>; TABLE_LEN] =
    [const { AtomicPtr::new(ptr::null_mut()) }; TABLE_LEN];

/// One-shot claim per shard, so two live `HandleTable`s can never share a
/// shard's chunk range.
///
/// This is not defensive paranoia about a case that cannot happen; it is what
/// makes "chunks are never freed" safe. A chunked table that were dropped would
/// leave its chunks published, and a second table constructed for the same shard
/// would inherit slots holding another table's `Entry`s. The process shards live
/// in a `OnceLock` static and are never dropped, so the claim always succeeds
/// exactly once — and if that ever stops being true, this asserts instead of
/// corrupting.
static CLAIMED: [AtomicBool; N_SHARDS] = [const { AtomicBool::new(false) }; N_SHARDS];

/// Address of the flat table, for [`super::slab_chunk_table_base`].
pub fn table_base() -> usize {
    CHUNK_TABLE.as_ptr() as usize
}

/// Take exclusive ownership of `shard`'s chunk range. Panics if it is already
/// owned — see `CLAIMED` for why that is the right response.
pub(super) fn claim_shard(shard: usize) {
    assert!(shard < N_SHARDS, "shard {shard} out of range");
    let prev = CLAIMED[shard].swap(true, Ordering::AcqRel);
    assert!(
        !prev,
        "heap::slab: shard {}'s chunk range was already claimed; a chunked \
         HandleTable must be a process-lifetime singleton (see chunks::CLAIMED)",
        shard
    );
}

/// Split a per-shard slot index into `(chunk, offset)`. Shift and mask.
#[inline]
pub(super) const fn split(idx: usize) -> (usize, usize) {
    (idx >> CHUNK_BITS, idx & CHUNK_MASK)
}

/// The published base of `(shard, chunk)`, or null when that chunk does not
/// exist yet.
///
/// `Acquire` pairs with the `Release` in `ensure_chunk`: a thread that
/// observes the pointer is guaranteed to observe the fully-initialized chunk it
/// points at. This is the ordering that lets a reader holding a valid handle
/// resolve an address with **no lock at all** — the whole point of 3.1.
#[inline]
pub(super) fn chunk_ptr(shard: usize, chunk: usize) -> *mut Slot {
    CHUNK_TABLE[shard * MAX_CHUNKS + chunk].load(Ordering::Acquire)
}

/// Address of slot `idx` in `shard`, or null when its chunk is unpublished.
///
/// This is the Rust twin of the IR sequence in [`super`]'s header, and it exists
/// so the two can be diffed by eye and tested against each other rather than
/// drifting.
#[inline]
pub(super) fn slot_addr(shard: usize, idx: usize) -> *mut Slot {
    let (chunk, off) = split(idx);
    if chunk >= MAX_CHUNKS {
        return ptr::null_mut();
    }
    let base = chunk_ptr(shard, chunk);
    if base.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `off < CHUNK_SLOTS` by construction of `split`, and a published
    // chunk always holds exactly `CHUNK_SLOTS` initialized slots.
    unsafe { base.add(off) }
}

/// Allocate and publish `(shard, chunk)` if it does not exist, returning its
/// base.
///
/// **Called only from the allocation path, already holding that shard's
/// `Mutex`.** It allocates (a `Box<[Slot]>`) and does nothing else: it never
/// calls `alloc_entry`, never takes another lock, never triggers a GC cycle.
/// That is the whole of its lock discipline, and it is why growth cannot become
/// a sixteenth re-entrancy bug.
///
/// The chunk is **fully initialized before publication**, and published with a
/// single `Release` store. It is then leaked deliberately: a chunk that could be
/// freed is a chunk whose addresses can dangle, which would give back the one
/// guarantee this module exists to provide.
pub(super) fn ensure_chunk(shard: usize, chunk: usize) -> *mut Slot {
    assert!(
        chunk < MAX_CHUNKS,
        "heap::slab: shard {} exhausted its {} chunks ({} slots); this is \
         unreachable below the HANDLES_MAX live-handle cap, so reaching it means \
         the cap was removed or the shard routing is wrong",
        shard,
        MAX_CHUNKS,
        MAX_CHUNKS * CHUNK_SLOTS
    );
    let cell = &CHUNK_TABLE[shard * MAX_CHUNKS + chunk];
    let existing = cell.load(Ordering::Acquire);
    if !existing.is_null() {
        return existing;
    }

    let mut slots: Vec<Slot> = Vec::with_capacity(CHUNK_SLOTS);
    for _ in 0..CHUNK_SLOTS {
        slots.push(Slot::free());
    }
    let boxed: Box<[Slot]> = slots.into_boxed_slice();
    let raw = Box::into_raw(boxed) as *mut Slot;
    // Single writer: the caller holds the shard's `Mutex`, and a shard's chunk
    // range is claimed by exactly one table. So a plain `Release` store is
    // enough — there is no competing publisher to CAS against, only readers to
    // synchronize with.
    cell.store(raw, Ordering::Release);
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_is_shift_and_mask() {
        assert_eq!(split(0), (0, 0));
        assert_eq!(split(CHUNK_SLOTS - 1), (0, CHUNK_SLOTS - 1));
        assert_eq!(split(CHUNK_SLOTS), (1, 0));
        assert_eq!(split(CHUNK_SLOTS * 3 + 7), (3, 7));
    }

    #[test]
    fn capacity_is_above_the_live_handle_cap() {
        // The exhaustion panic must be unreachable behind `HANDLES_MAX` (5 M).
        // Asserted rather than commented so a future shrink of MAX_CHUNKS has to
        // confront it.
        assert!(MAX_CHUNKS * CHUNK_SLOTS > 5_000_000);
    }

    #[test]
    fn table_base_is_stable() {
        assert_eq!(table_base(), table_base());
        assert_ne!(table_base(), 0);
    }
}
