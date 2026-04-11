//! GC Arena — arena-based allocation with deterministic collection via gc-arena.
//!
//! Each execution thread owns a thread-local `GcArena`. Values are allocated as
//! tagged blobs (`GcBlob`) and referenced by stable u64 handles. Setting a
//! handle to `None` makes the blob unreachable; the next `collect_*` call
//! frees the backing memory.
//!
//! # Safe points
//! Call `collect_all()` only when no `Gc` pointers are live on the call stack
//! (i.e. at quiescence: after function return, after class method, after
//! closure scope). Use `collect_debt()` for amortised work at minor points.

use std::cell::RefCell;

use gc_arena::{Arena, Collect, Gc, Rootable};

// ── Blob kind tags ──────────────────────────────────────────────────────────

pub const KIND_NULL: u8 = 0;
pub const KIND_BOOL: u8 = 1;
pub const KIND_NUMBER: u8 = 2;
pub const KIND_STRING: u8 = 3;
pub const KIND_OBJECT: u8 = 4;
pub const KIND_ARRAY: u8 = 5;
pub const KIND_BYTES: u8 = 6;

// ── Heap value ──────────────────────────────────────────────────────────────

/// A tagged byte-blob managed by the GC arena.
/// All JS heap values are serialised into this representation.
///
/// `require_static` is correct: `GcBlob` contains no `Gc<'gc, _>` pointers,
/// so tracing is a no-op and the value is safe to allocate as a leaf node.
#[derive(Collect, Clone, Debug)]
#[collect(require_static)]
pub struct GcBlob {
    pub kind: u8,
    pub payload: Vec<u8>,
}

impl GcBlob {
    #[inline]
    pub fn new(kind: u8, payload: Vec<u8>) -> Self {
        Self { kind, payload }
    }

    pub fn null() -> Self {
        Self::new(KIND_NULL, vec![])
    }
    pub fn bool(v: bool) -> Self {
        Self::new(KIND_BOOL, vec![v as u8])
    }
    pub fn number(v: f64) -> Self {
        Self::new(KIND_NUMBER, v.to_le_bytes().to_vec())
    }
    pub fn string(v: &str) -> Self {
        Self::new(KIND_STRING, v.as_bytes().to_vec())
    }
    pub fn bytes(v: &[u8]) -> Self {
        Self::new(KIND_BYTES, v.to_vec())
    }
}

// ── Arena root ──────────────────────────────────────────────────────────────

/// GC root: owns all live slots. Slots set to `None` are unreachable and will
/// be freed on the next collection pass.
#[derive(Collect)]
#[collect(no_drop)]
struct GcPool<'gc> {
    slots: Vec<Option<Gc<'gc, GcBlob>>>,
    /// Slot indices available for reuse.
    free_list: Vec<u64>,
}

impl<'gc> GcPool<'gc> {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_list: Vec::new(),
        }
    }
}

type RtsArena = Arena<Rootable![GcPool<'_>]>;

// ── Arena wrapper ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default)]
pub struct GcStats {
    /// Running total of bytes passed to `alloc` (monotonically increasing).
    pub allocated_bytes: usize,
    /// Number of full-collection passes completed.
    pub generation: u64,
    /// Current number of live slots.
    pub live_slots: usize,
}

pub struct GcArena {
    arena: RtsArena,
    allocated_bytes: usize,
    generation: u64,
}

impl GcArena {
    pub fn new() -> Self {
        Self {
            arena: RtsArena::new(|_mc| GcPool::new()),
            allocated_bytes: 0,
            generation: 0,
        }
    }

    /// Allocate a blob and return a stable handle (u64 index).
    pub fn alloc(&mut self, blob: GcBlob) -> u64 {
        self.allocated_bytes += blob.payload.len() + std::mem::size_of::<GcBlob>();

        self.arena.mutate_root(|mc, pool| {
            let gc_blob = Gc::new(mc, blob);

            if let Some(idx) = pool.free_list.pop() {
                pool.slots[idx as usize] = Some(gc_blob);
                idx
            } else {
                let idx = pool.slots.len() as u64;
                pool.slots.push(Some(gc_blob));
                idx
            }
        })
    }

    /// Clone the blob at `handle`, if still live.
    pub fn get(&self, handle: u64) -> Option<GcBlob> {
        self.arena.mutate(|_mc, pool| {
            pool.slots
                .get(handle as usize)?
                .as_ref()
                .map(|gc| (**gc).clone())
        })
    }

    /// Release `handle`: marks the blob unreachable for the next collection.
    /// Returns `false` if the handle was already free or out of range.
    pub fn free(&mut self, handle: u64) -> bool {
        self.arena
            .mutate_root(|_mc, pool| match pool.slots.get_mut(handle as usize) {
                Some(slot @ &mut Some(_)) => {
                    *slot = None;
                    pool.free_list.push(handle);
                    true
                }
                _ => false,
            })
    }

    /// Amortised GC — collect proportional to allocation debt.
    /// Use at minor quiescence points (e.g. between loop iterations).
    pub fn collect_debt(&mut self) {
        self.arena.collect_debt();
    }

    /// Full GC pass — collect all unreachable blobs.
    /// **Only safe at major quiescence points** (function return, class end,
    /// closure scope end) where no `Gc` pointers live on the stack.
    pub fn collect_all(&mut self) {
        self.arena.collect_all();
        self.generation += 1;
    }

    pub fn stats(&self) -> GcStats {
        let live_slots = self
            .arena
            .mutate(|_mc, pool| pool.slots.iter().filter(|s| s.is_some()).count());
        GcStats {
            allocated_bytes: self.allocated_bytes,
            generation: self.generation,
            live_slots,
        }
    }
}

impl Default for GcArena {
    fn default() -> Self {
        Self::new()
    }
}

// ── Thread-local arena ──────────────────────────────────────────────────────

thread_local! {
    static ARENA: RefCell<GcArena> = RefCell::new(GcArena::new());
}

/// Allocate a blob into the thread-local arena. Returns a handle.
pub fn alloc(blob: GcBlob) -> u64 {
    ARENA.with(|a| a.borrow_mut().alloc(blob))
}

/// Read a blob by handle (clones the value out of the arena).
pub fn get(handle: u64) -> Option<GcBlob> {
    ARENA.with(|a| a.borrow().get(handle))
}

/// Release a handle.
pub fn free(handle: u64) -> bool {
    ARENA.with(|a| a.borrow_mut().free(handle))
}

/// Amortised debt collection.
pub fn collect_debt() {
    ARENA.with(|a| a.borrow_mut().collect_debt());
}

/// Full collection — only call at safe quiescence points.
pub fn collect_all() {
    ARENA.with(|a| a.borrow_mut().collect_all());
}

/// Current GC statistics for the calling thread.
pub fn stats() -> GcStats {
    ARENA.with(|a| a.borrow().stats())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_get_free_roundtrip() {
        let mut arena = GcArena::new();

        let h = arena.alloc(GcBlob::string("hello"));
        let blob = arena.get(h).expect("handle must be live");
        assert_eq!(blob.kind, KIND_STRING);
        assert_eq!(&*blob.payload, b"hello");

        assert!(arena.free(h));
        assert!(!arena.free(h)); // double-free returns false

        arena.collect_all();
        assert!(arena.get(h).is_none()); // slot is gone after collect
    }

    #[test]
    fn slot_reuse_after_free() {
        let mut arena = GcArena::new();

        let h1 = arena.alloc(GcBlob::number(1.0));
        arena.free(h1);
        arena.collect_all();

        let h2 = arena.alloc(GcBlob::number(2.0));
        // free list pops h1's slot for reuse
        assert_eq!(h2, h1);
    }

    #[test]
    fn generation_increments_on_collect_all() {
        let mut arena = GcArena::new();
        assert_eq!(arena.stats().generation, 0);
        arena.collect_all();
        assert_eq!(arena.stats().generation, 1);
    }
}
