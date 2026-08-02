//! [`Slot`] and [`SlotStore`] — the two-path storage `HandleTable` holds.
//!
//! `SlotStore` is a drop-in replacement for the `Vec<Slot>` that `HandleTable`
//! used to own: same operations, same index semantics, same iteration order. The
//! `Vec` variant IS that `Vec`; the `Chunked` variant is Tier 3.1's
//! stable-address storage. See [`super`] for why, and for the ABA and GC
//! arguments.

use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};

use crate::heap::handles::Entry;

use super::chunks;

/// One handle-table slot.
///
/// Moved here from `handles.rs` (a 2 700-line draining target that may only
/// shrink) because the storage is what owns its layout now: `size_of::<Slot>()`
/// is the stride Tier 3.2's address computation multiplies by, and
/// `RTS_CLASS_IMPLEMENTATION.md` §6.3 requires exactly one definition of a
/// layout constant, in `rts-natives`, imported by the codegen.
#[derive(Debug)]
pub struct Slot {
    /// Bumped on every reuse of this slot; the ABA answer (see [`super`]).
    ///
    /// **Atomic on purpose, not for contention.** A stable address means a
    /// reader can compute where this slot lives without taking the shard
    /// `Mutex`; validating a handle against a plain `u16` while another thread
    /// runs `set_generation` under the lock would be a data race in the Rust
    /// memory model, whatever the machine does. `Relaxed` compiles to the same
    /// load, so making the read well-defined costs nothing. It is a rejection
    /// test and never publishes data — the ordering argument is in [`super`].
    generation: AtomicU16,
    /// Set during the GC mark phase. Cleared at sweep. Only ever touched under
    /// the shard `Mutex`, so a plain `bool` is correct and must stay one — a
    /// lock-free reader has no business reading a mark bit.
    pub(crate) marked: bool,
    pub(crate) entry: Entry,
}

impl Slot {
    /// A fresh, unused slot. Generation `0` is the sentinel that
    /// `next_generation` skips, so a never-allocated slot can never match a real
    /// handle's generation.
    pub fn free() -> Self {
        Slot {
            generation: AtomicU16::new(0),
            marked: false,
            entry: Entry::Free,
        }
    }

    /// A slot holding `entry` at generation `generation`.
    pub(crate) fn live(generation: u16, entry: Entry) -> Self {
        Slot {
            generation: AtomicU16::new(generation),
            marked: false,
            entry,
        }
    }

    #[inline]
    pub(crate) fn generation(&self) -> u16 {
        self.generation.load(Ordering::Relaxed)
    }

    #[inline]
    pub(crate) fn set_generation(&self, generation: u16) {
        self.generation.store(generation, Ordering::Relaxed);
    }
}

/// Per-shard slot storage. `Vec` is the default; `Chunked` is `RTS_SLAB=1`.
pub enum SlotStore {
    /// Today's storage, byte-for-byte. Reallocates, so no address into it
    /// survives a `push`.
    Vec(Vec<Slot>),
    /// Tier 3.1: chunks never move, only the chunk list grows.
    Chunked(Chunked),
}

impl std::fmt::Debug for SlotStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately does not print the slots: `HandleTable`'s derived `Debug`
        // is used in panic paths, and formatting millions of `Entry`s there is a
        // hazard, not a diagnostic.
        let kind = match self {
            SlotStore::Vec(_) => "Vec",
            SlotStore::Chunked(_) => "Chunked",
        };
        f.debug_struct("SlotStore")
            .field("kind", &kind)
            .field("len", &self.len())
            .finish()
    }
}

impl Default for SlotStore {
    /// **Always `Vec`.** A `Chunked` store owns a shard's range of the global
    /// chunk table for the life of the process (see `chunks::CLAIMED`), and
    /// `Default` has no shard identity to own one with. The process's real
    /// shards are built by [`SlotStore::for_shard`], which honours the knob;
    /// `default()` is the constructor for standalone/test tables and stays on
    /// the historical path.
    fn default() -> Self {
        SlotStore::Vec(Vec::new())
    }
}

impl SlotStore {
    /// The store for shard `shard_idx`: chunked when `RTS_SLAB=1`, otherwise the
    /// historical `Vec`. Call exactly once per shard, at process-shard
    /// construction.
    pub fn for_shard(shard_idx: usize) -> Self {
        if super::enabled() {
            SlotStore::Chunked(Chunked::new(shard_idx))
        } else {
            SlotStore::Vec(Vec::new())
        }
    }

    /// Number of slots ever appended, freed ones included. Both paths agree:
    /// freeing pushes an index onto `free_list` and never shrinks this.
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            SlotStore::Vec(v) => v.len(),
            SlotStore::Chunked(c) => c.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<&Slot> {
        match self {
            SlotStore::Vec(v) => v.get(idx),
            SlotStore::Chunked(c) => c.get(idx),
        }
    }

    #[inline]
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut Slot> {
        match self {
            SlotStore::Vec(v) => v.get_mut(idx),
            SlotStore::Chunked(c) => c.get_mut(idx),
        }
    }

    /// Append a slot at index `len()`. Both paths assign indices densely and in
    /// the same order — invariant #1 of [`super`]'s agreement table.
    #[inline]
    pub fn push(&mut self, slot: Slot) {
        match self {
            SlotStore::Vec(v) => v.push(slot),
            SlotStore::Chunked(c) => c.push(slot),
        }
    }

    /// Ascending by index, in both paths — invariant #3. The sweep and
    /// `live_handles_snapshot` depend on this, so it is not an implementation
    /// detail.
    pub fn iter(&self) -> Iter<'_> {
        Iter { store: self, idx: 0 }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_> {
        let len = self.len();
        IterMut {
            store: self,
            idx: 0,
            len,
        }
    }
}

pub struct Iter<'a> {
    store: &'a SlotStore,
    idx: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Slot;

    fn next(&mut self) -> Option<Self::Item> {
        let out = self.store.get(self.idx)?;
        self.idx += 1;
        Some(out)
    }
}

pub struct IterMut<'a> {
    store: &'a mut SlotStore,
    idx: usize,
    len: usize,
}

impl<'a> Iterator for IterMut<'a> {
    type Item = &'a mut Slot;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.len {
            return None;
        }
        let slot = self.store.get_mut(self.idx)?;
        self.idx += 1;
        // SAFETY: `IterMut` holds `&mut SlotStore` for `'a`, and each index is
        // yielded at most once (`idx` only increases), so no two live references
        // alias. This is the standard slice-`iter_mut` argument; it needs to be
        // written by hand only because the chunked store is not a slice.
        Some(unsafe { &mut *(slot as *mut Slot) })
    }
}

/// Chunked, append-only, stable-address storage for one shard.
///
/// Owns no memory itself: the chunks live in the process-global table
/// ([`chunks`]) and are never freed. What this holds is the shard identity and
/// the high-water mark.
pub struct Chunked {
    shard: usize,
    /// Slots appended so far.
    ///
    /// Atomic for the same reason the generation is: a lock-free reader must be
    /// able to bounds-check an index it did not get from under the lock. Written
    /// `Release` *after* the slot's contents are in place, so a reader that sees
    /// the new length sees an initialized slot.
    len: AtomicUsize,
}

impl Chunked {
    fn new(shard: usize) -> Self {
        chunks::claim_shard(shard);
        Chunked {
            shard,
            len: AtomicUsize::new(0),
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }

    #[inline]
    fn get(&self, idx: usize) -> Option<&Slot> {
        if idx >= self.len() {
            return None;
        }
        let p = chunks::slot_addr(self.shard, idx);
        if p.is_null() {
            return None;
        }
        // SAFETY: `idx < len` means the slot was published by `push`, and a
        // published chunk is initialized and never freed or moved.
        Some(unsafe { &*p })
    }

    #[inline]
    fn get_mut(&mut self, idx: usize) -> Option<&mut Slot> {
        if idx >= self.len() {
            return None;
        }
        let p = chunks::slot_addr(self.shard, idx);
        if p.is_null() {
            return None;
        }
        // SAFETY: as `get`, plus `&mut self` — which, since a shard's `Chunked`
        // is reachable only through that shard's `Mutex`, is exclusive access to
        // every slot in the shard.
        Some(unsafe { &mut *p })
    }

    fn push(&mut self, slot: Slot) {
        let idx = self.len.load(Ordering::Relaxed);
        let (chunk, off) = chunks::split(idx);
        // Grow first: allocation happens here, under the shard `Mutex` the
        // caller already holds, and touches nothing but the global allocator.
        let base = chunks::ensure_chunk(self.shard, chunk);
        // SAFETY: `ensure_chunk` returned an initialized chunk of `CHUNK_SLOTS`
        // slots and `off < CHUNK_SLOTS`.
        unsafe {
            *base.add(off) = slot;
        }
        // Publish AFTER the write: a reader that observes this length observes
        // an initialized slot.
        self.len.store(idx + 1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chunked path can only be exercised where a shard range is free to
    /// claim, and `chunks::CLAIMED` is process-global — so tests take a shard
    /// from the TOP of the range, which the runtime's own shards (built from
    /// index 0 upward, and only when `RTS_SLAB=1`) will not have taken in a test
    /// binary. If this ever collides, `claim_shard` panics loudly rather than
    /// silently sharing.
    fn test_store(shard: usize) -> SlotStore {
        SlotStore::Chunked(Chunked::new(shard))
    }

    fn fill(store: &mut SlotStore, n: usize) {
        for i in 0..n {
            store.push(Slot::live((i % 0xFFF7).max(1) as u16, Entry::Free));
        }
    }

    #[test]
    fn indices_are_dense_and_ordered_across_chunk_boundaries() {
        let mut s = test_store(31);
        let n = chunks::CHUNK_SLOTS * 2 + 5;
        fill(&mut s, n);
        assert_eq!(s.len(), n);
        for (i, slot) in s.iter().enumerate() {
            assert_eq!(slot.generation(), ((i % 0xFFF7).max(1)) as u16, "index {i}");
        }
        assert!(s.get(n).is_none(), "out of range must be None, not a panic");
    }

    #[test]
    fn addresses_are_stable_across_growth() {
        // The property the whole item exists for: an address taken before the
        // store grows past several chunk boundaries is still the same slot after.
        let mut s = test_store(30);
        fill(&mut s, 3);
        let before = s.get(1).unwrap() as *const Slot;
        fill(&mut s, chunks::CHUNK_SLOTS * 3);
        let after = s.get(1).unwrap() as *const Slot;
        assert_eq!(before, after, "a chunked slot address must never move");
    }

    #[test]
    fn vec_and_chunked_agree_on_iteration_order() {
        let mut a = SlotStore::default();
        let mut b = test_store(29);
        fill(&mut a, chunks::CHUNK_SLOTS + 3);
        fill(&mut b, chunks::CHUNK_SLOTS + 3);
        assert_eq!(a.len(), b.len());
        let ga: Vec<u16> = a.iter().map(|s| s.generation()).collect();
        let gb: Vec<u16> = b.iter().map(|s| s.generation()).collect();
        assert_eq!(ga, gb);
    }

    #[test]
    fn iter_mut_visits_every_slot_once() {
        let mut s = test_store(28);
        fill(&mut s, chunks::CHUNK_SLOTS + 1);
        let mut n = 0;
        for slot in s.iter_mut() {
            slot.marked = true;
            n += 1;
        }
        assert_eq!(n, chunks::CHUNK_SLOTS + 1);
        assert!(s.iter().all(|s| s.marked));
    }

    #[test]
    fn default_is_the_historical_path() {
        assert!(matches!(SlotStore::default(), SlotStore::Vec(_)));
    }
}
