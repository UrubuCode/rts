//! The three heap shapes under test.
//!
//! 1. [`sharded`] — the CURRENT design, replicated: 32 shards, each a
//!    `Mutex<Vec<Slot>>`, `Slot { generation: u16, marked: bool, entry: Entry }`,
//!    payload routed by `(payload & 31)` → shard, `payload >> 5` → table slot,
//!    exactly like `rts-natives/src/heap/payload_ops.rs::with_payload_slot`.
//! 2. [`unlocked`] — byte-identical layout with the `Mutex` removed. Isolates the
//!    LOCK cost from the CALL cost and from the indirection cost.
//! 3. [`arena`] — a flat `Vec<i64>` with a stable base, objects laid out inline
//!    (`[shape_id, f0, f1, ...]`). This is the V8-pointer-compression shape: the
//!    payload is an OFFSET, so a field address is `base + (payload+1+slot)*8` —
//!    computable in pure Cranelift IR, no call, no lock.
//!
//! The probe is single-threaded by construction (see `main.rs`), which is what
//! makes the `unlocked` and `arena` variants sound here. That is also the honest
//! limitation: they measure the cost the lock imposes on a single thread, not a
//! design that would be correct under RTS's real multi-thread surface.

use std::cell::UnsafeCell;
use std::sync::{Mutex, OnceLock};

pub const N_SHARDS: usize = 32;
pub const SHARD_BITS: u32 = 5;
pub const SHARD_MASK: u64 = (N_SHARDS as u64) - 1;

/// Stand-in for `rts-natives`'s ~50-variant `Entry`. The `Pad` variant exists so
/// `size_of::<Slot>()` lands in the same ballpark as the real one — the slab is a
/// Vec of INLINE slots, so the widest variant sets the stride and therefore the
/// cache behaviour. `main.rs` prints the size so the assumption is visible.
// `Free` and `Pad` are never constructed and `generation`/`marked` are never
// read: they are here for LAYOUT FIDELITY, not for behaviour. The real slab is a
// `Vec<Slot>` of inline slots, so the widest `Entry` variant sets the stride and
// therefore the cache behaviour of every field read under test. Deleting them
// would make the locked variants look better than the real thing.
#[allow(dead_code)]
pub enum Entry {
    Free,
    Vec(Box<Vec<i64>>),
    /// `Entry::String(Vec<u8>)` — verbatim from `handles.rs:395`.
    String(Vec<u8>),
    /// The dictionary object representation (`handles.rs:409`).
    Map(Box<indexmap::IndexMap<String, i64>>),
    Pad([u8; 40]),
}

#[allow(dead_code)]
pub struct Slot {
    pub generation: u16,
    pub marked: bool,
    pub entry: Entry,
}

// ---------------------------------------------------------------------------
// 1. sharded + Mutex — the current design
// ---------------------------------------------------------------------------

pub mod sharded {
    use super::*;

    #[allow(clippy::type_complexity)]
    fn shards() -> &'static [Mutex<Vec<Slot>>; N_SHARDS] {
        static S: OnceLock<[Mutex<Vec<Slot>>; N_SHARDS]> = OnceLock::new();
        S.get_or_init(|| std::array::from_fn(|_| Mutex::new(Vec::new())))
    }

    /// Round-robin across shards, exactly like `alloc_entry`'s `ALLOC_SHARD`.
    pub fn alloc_object(fields: &[i64], shape_id: i64) -> u64 {
        use std::cell::Cell;
        thread_local! { static NEXT: Cell<usize> = const { Cell::new(0) }; }
        let shard_idx = NEXT.with(|c| {
            let v = c.get();
            c.set((v + 1) % N_SHARDS);
            v
        });
        let mut v = Vec::with_capacity(fields.len() + 1);
        v.push(shape_id);
        v.extend_from_slice(fields);
        let mut guard = shards()[shard_idx].lock().unwrap();
        let table_slot = guard.len() as u64;
        guard.push(Slot {
            generation: 1,
            marked: false,
            entry: Entry::Vec(Box::new(v)),
        });
        (table_slot << SHARD_BITS) | shard_idx as u64
    }

    #[inline]
    pub fn vec_get(payload: u64, index: i64) -> i64 {
        let shard_idx = (payload & SHARD_MASK) as usize;
        let table_slot = (payload >> SHARD_BITS) as usize;
        let guard = shards()[shard_idx].lock().unwrap();
        match guard.get(table_slot) {
            Some(Slot {
                entry: Entry::Vec(v),
                ..
            }) => v.get(index as usize).copied().unwrap_or(0),
            _ => 0,
        }
    }

    #[inline]
    pub fn vec_set(payload: u64, index: i64, value: i64) {
        if index < 0 {
            return;
        }
        let shard_idx = (payload & SHARD_MASK) as usize;
        let table_slot = (payload >> SHARD_BITS) as usize;
        let mut guard = shards()[shard_idx].lock().unwrap();
        if let Some(Slot {
            entry: Entry::Vec(v),
            ..
        }) = guard.get_mut(table_slot)
            && let Some(slot) = v.get_mut(index as usize)
        {
            *slot = value;
        }
    }

    /// Allocate any `Entry` (string / map / vec) and return its 48-bit payload.
    pub fn alloc(entry: Entry) -> u64 {
        use std::cell::Cell;
        thread_local! { static NEXT: Cell<usize> = const { Cell::new(0) }; }
        let shard_idx = NEXT.with(|c| {
            let v = c.get();
            c.set((v + 1) % N_SHARDS);
            v
        });
        let mut guard = shards()[shard_idx].lock().unwrap();
        let table_slot = guard.len() as u64;
        guard.push(Slot {
            generation: 1,
            marked: false,
            entry,
        });
        (table_slot << SHARD_BITS) | shard_idx as u64
    }

    /// `with_entry` — one shard lock, `f` sees the entry.
    #[inline]
    pub fn with<R>(payload: u64, f: impl FnOnce(Option<&Entry>) -> R) -> R {
        let shard_idx = (payload & SHARD_MASK) as usize;
        let table_slot = (payload >> SHARD_BITS) as usize;
        let guard = shards()[shard_idx].lock().unwrap();
        f(guard.get(table_slot).map(|s| &s.entry))
    }

    #[inline]
    pub fn with_mut<R>(payload: u64, f: impl FnOnce(Option<&mut Entry>) -> R) -> R {
        let shard_idx = (payload & SHARD_MASK) as usize;
        let table_slot = (payload >> SHARD_BITS) as usize;
        let mut guard = shards()[shard_idx].lock().unwrap();
        f(guard.get_mut(table_slot).map(|s| &mut s.entry))
    }

    /// `with_two_entries` (`handles.rs:1965`): both entries under ONE lock when
    /// they share a shard, two locks otherwise. Replicated because the naive
    /// alternative — read one, clone it, then read the other — adds an
    /// allocation the real code does not pay, which would make the current
    /// string-equality path look worse than it is.
    #[inline]
    pub fn with_two<R>(
        pa: u64,
        pb: u64,
        f: impl FnOnce(Option<&Entry>, Option<&Entry>) -> R,
    ) -> R {
        let (sa, ta) = ((pa & SHARD_MASK) as usize, (pa >> SHARD_BITS) as usize);
        let (sb, tb) = ((pb & SHARD_MASK) as usize, (pb >> SHARD_BITS) as usize);
        if sa == sb {
            let g = shards()[sa].lock().unwrap();
            let ea = g.get(ta).map(|s| &s.entry);
            let eb = g.get(tb).map(|s| &s.entry);
            return f(ea, eb);
        }
        // Lock the lower shard index first — the real one does the same to keep
        // a consistent order and avoid a deadlock between two threads.
        let (lo, hi) = if sa < sb { (sa, sb) } else { (sb, sa) };
        let g_lo = shards()[lo].lock().unwrap();
        let g_hi = shards()[hi].lock().unwrap();
        let (ga, gb) = if sa < sb { (&g_lo, &g_hi) } else { (&g_hi, &g_lo) };
        f(ga.get(ta).map(|s| &s.entry), gb.get(tb).map(|s| &s.entry))
    }

    /// Sweep only when the live count is past `floor`, as `alloc_entry` gates
    /// `finish_cycle` on `GC_LIVE_FLOOR`. Reclaims as it goes, so the slab does
    /// not grow without bound and later sweeps do not get monotonically slower.
    pub fn sweep_if_past_floor(floor: usize) -> i64 {
        let total: usize = shards().iter().map(|s| s.lock().unwrap().len()).sum();
        if total < floor {
            return 0;
        }
        let mut live = 0i64;
        for s in shards().iter() {
            let mut g = s.lock().unwrap();
            for slot in g.iter_mut() {
                if slot.entry.is_live() {
                    live += 1;
                    slot.entry = Entry::Free;
                }
            }
            g.clear();
        }
        live
    }

    /// Walk every slot of every shard. Returns the live count so the work
    /// cannot be optimised away. Kept alongside `sweep_if_past_floor` because
    /// the DIFFERENCE between them is a finding: a floorless, non-reclaiming
    /// sweep overshot the engine by 14.6x, which is what an under-triggering
    /// live-bytes counter would produce.
    #[allow(dead_code)]
    pub fn sweep_all() -> i64 {
        let mut live = 0i64;
        for s in shards().iter() {
            let g = s.lock().unwrap();
            for slot in g.iter() {
                if slot.entry.is_live() {
                    live += 1;
                }
            }
        }
        live
    }

    /// Drop every slot — stands in for a GC sweep, so the allocation kernel can
    /// be run repeatedly without the slab growing across runs.
    pub fn reset() {
        for s in shards().iter() {
            s.lock().unwrap().clear();
        }
    }
}

// ---------------------------------------------------------------------------
// 2. same layout, no Mutex — isolates the lock
// ---------------------------------------------------------------------------

pub mod unlocked {
    use super::*;

    struct Shards(UnsafeCell<Vec<Vec<Slot>>>);
    // SAFETY: the probe runs every kernel on one thread (asserted in main.rs).
    unsafe impl Sync for Shards {}

    fn shards() -> &'static Shards {
        static S: OnceLock<Shards> = OnceLock::new();
        S.get_or_init(|| Shards(UnsafeCell::new((0..N_SHARDS).map(|_| Vec::new()).collect())))
    }

    pub fn alloc_object(fields: &[i64], shape_id: i64) -> u64 {
        use std::cell::Cell;
        thread_local! { static NEXT: Cell<usize> = const { Cell::new(0) }; }
        let shard_idx = NEXT.with(|c| {
            let v = c.get();
            c.set((v + 1) % N_SHARDS);
            v
        });
        let mut v = Vec::with_capacity(fields.len() + 1);
        v.push(shape_id);
        v.extend_from_slice(fields);
        // SAFETY: single-threaded probe; no live borrow escapes this call.
        let all = unsafe { &mut *shards().0.get() };
        let table_slot = all[shard_idx].len() as u64;
        all[shard_idx].push(Slot {
            generation: 1,
            marked: false,
            entry: Entry::Vec(Box::new(v)),
        });
        (table_slot << SHARD_BITS) | shard_idx as u64
    }

    #[inline]
    pub fn vec_get(payload: u64, index: i64) -> i64 {
        let shard_idx = (payload & SHARD_MASK) as usize;
        let table_slot = (payload >> SHARD_BITS) as usize;
        // SAFETY: single-threaded probe, read-only.
        let all = unsafe { &*shards().0.get() };
        match all[shard_idx].get(table_slot) {
            Some(Slot {
                entry: Entry::Vec(v),
                ..
            }) => v.get(index as usize).copied().unwrap_or(0),
            _ => 0,
        }
    }

    pub fn reset() {
        // SAFETY: single-threaded probe.
        let all = unsafe { &mut *shards().0.get() };
        for s in all.iter_mut() {
            s.clear();
        }
    }
}

// ---------------------------------------------------------------------------
// 3. flat arena with a stable base — the pointer-compression shape
// ---------------------------------------------------------------------------

pub mod arena {
    use super::*;

    /// Pre-reserved so the base pointer never moves; a realloc mid-run would
    /// invalidate the base the JIT baked in.
    const CAP: usize = 16 * 1024 * 1024;

    struct Arena(UnsafeCell<Vec<i64>>);
    // SAFETY: single-threaded probe.
    unsafe impl Sync for Arena {}

    fn arena() -> &'static Arena {
        static A: OnceLock<Arena> = OnceLock::new();
        A.get_or_init(|| Arena(UnsafeCell::new(Vec::with_capacity(CAP))))
    }

    /// Bump-allocate `[shape_id, fields...]`; returns the ELEMENT offset.
    pub fn alloc_object(fields: &[i64], shape_id: i64) -> u64 {
        // SAFETY: single-threaded probe.
        let v = unsafe { &mut *arena().0.get() };
        assert!(
            v.len() + fields.len() + 1 <= CAP,
            "probe arena exhausted — raise CAP"
        );
        let off = v.len() as u64;
        v.push(shape_id);
        v.extend_from_slice(fields);
        off
    }

    /// Bump-allocate an arbitrary word sequence with NO implied header — the
    /// struct layouts (`[x][y]` and `[class_id][x][y]`) need to control their
    /// own first word, which `alloc_object` does not allow.
    pub fn alloc_raw(words: &[i64]) -> u64 {
        // SAFETY: single-threaded probe.
        let v = unsafe { &mut *arena().0.get() };
        assert!(v.len() + words.len() <= CAP, "probe arena exhausted");
        let off = v.len() as u64;
        v.extend_from_slice(words);
        off
    }

    /// Reset the bump pointer between kernels so the alloc benchmark can run
    /// repeatedly without exhausting the reservation.
    pub fn reset() {
        // SAFETY: single-threaded probe.
        let v = unsafe { &mut *arena().0.get() };
        v.clear();
    }

    /// The base address the JIT bakes in as an `iconst` — this is exactly what
    /// V8's pointer compression holds in a reserved register.
    pub fn base_addr() -> i64 {
        // SAFETY: single-threaded probe; capacity is reserved so this is stable.
        let v = unsafe { &*arena().0.get() };
        v.as_ptr() as i64
    }
}

/// Sanity: the probe's payload encoding round-trips like the real one.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_routes_like_the_runtime() {
        let payload = (12345u64 << SHARD_BITS) | 7;
        assert_eq!(payload & SHARD_MASK, 7);
        assert_eq!(payload >> SHARD_BITS, 12345);
    }

    #[test]
    fn sharded_and_unlocked_agree() {
        let a = sharded::alloc_object(&[10, 20], 99);
        let b = unlocked::alloc_object(&[10, 20], 99);
        assert_eq!(sharded::vec_get(a, 0), 99);
        assert_eq!(sharded::vec_get(a, 1), 10);
        assert_eq!(sharded::vec_get(a, 2), 20);
        assert_eq!(unlocked::vec_get(b, 0), 99);
        assert_eq!(unlocked::vec_get(b, 1), 10);
        assert_eq!(unlocked::vec_get(b, 2), 20);
    }
}

impl Entry {
    /// Cheap "is this slot live" probe used by the sweep replica.
    fn is_live(&self) -> bool {
        !matches!(self, Entry::Free)
    }
}
