//! Module-global CELLS — shared across a program's threads (threading-model T1).
//!
//! A top-level `let`/`const` that a function writes is promoted by the engine's
//! front-end to a CELL: every access (top level + the capturing function) goes
//! through `GCELL_GET`/`GCELL_SET` by a compile-time id. In JS a module-level
//! binding is ONE binding for the whole program — every thread must see the same
//! cell, and a write from any thread must be visible to all.
//!
//! ## Why this is not a thread_local (what T1 fixes)
//!
//! It used to be: `thread_local! { static GCELLS: RefCell<Vec<u64>> }`, with
//! `thread.spawn`/`parallel`/the async spawn SNAPSHOTTING the values into the
//! worker. That copy made a worker's write silently local — `let n = 0;` +
//! `thread.spawn(() => n++)` incremented a copy and dropped it — which is
//! blocker #1 of docs/specs/rts-threading-model.md ("promote gcells to shared
//! cells with synchronized writes"), and the reason `setInterval`'s callback had
//! to be routed back to the main thread by hand.
//!
//! ## Program isolation (why it is not a plain global either)
//!
//! Cell ids restart at 0 for each compiled program, and several programs can run
//! CONCURRENTLY in one process (the parallel unit tests). So the store is
//! per-PROGRAM: each program owns an [`Arc<GcellStore>`], and each of its threads
//! points at it ([`share`]/[`attach`], used where the old snapshot/restore was).
//! A thread that runs a program without being handed one creates its own, so two
//! programs on two threads are isolated exactly as the old thread_local made them
//! — what changes is that a program's OWN threads now share one store instead of
//! each getting a copy. Two programs run sequentially on ONE thread reuse that
//! thread's store, which is also what the thread_local did (it was never cleared):
//! safe because the front-end always SETs a cell (the initializer) before any GET.
//!
//! ## Shape: lock-free reads, no reallocation
//!
//! `GCELL_GET` is on the hot path (every module-global read in JIT'd code), so
//! the store is an append-only array of fixed CHUNKS: reading is a chunk load +
//! an atomic load, with no lock and no `RefCell` — cheaper than the thread_local
//! it replaces. A chunk is allocated once, on first touch, and never moves, so a
//! concurrent reader can never see a reallocation. A cell is a 64-bit PolyValue
//! word, which is exactly the "64-bit word = atomic load/store, never tears"
//! property the threading model is built on (§"Why PolyValue supports it").

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

/// Cells per chunk. A chunk is allocated on first touch; programs have tens to
/// low thousands of module globals, so the first chunk usually covers everything.
const CHUNK: usize = 1024;

/// Chunks per store — the cell-id ceiling (`CHUNK * MAX_CHUNKS`). Far above any
/// real program's module-global count; an id past it is a front-end bug, and the
/// accessors report it rather than corrupting memory.
const MAX_CHUNKS: usize = 64;

/// One program's cells, shared by every thread of that program.
pub struct GcellStore {
    chunks: [OnceLock<Box<[AtomicU64; CHUNK]>>; MAX_CHUNKS],
    /// Highest touched id + 1 — the bound `mark_gcell_roots` scans to.
    high: AtomicUsize,
}

impl GcellStore {
    fn new() -> Self {
        Self {
            chunks: [const { OnceLock::new() }; MAX_CHUNKS],
            high: AtomicUsize::new(0),
        }
    }

    /// The cell at `id`, allocating its chunk on first touch. `None` only for an
    /// id past the ceiling.
    fn cell(&self, id: usize) -> Option<&AtomicU64> {
        let (chunk, slot) = (id / CHUNK, id % CHUNK);
        let chunk = self.chunks.get(chunk)?;
        // `get_or_init` races benignly: one initializer wins, both threads then
        // read the same chunk.
        let cells = chunk.get_or_init(|| Box::new([const { AtomicU64::new(0) }; CHUNK]));
        Some(&cells[slot])
    }

    pub fn get(&self, id: u64) -> u64 {
        // A never-SET cell reads 0 — the front-end always SETs (the initializer)
        // before any GET, so a well-formed program never observes it.
        self.cell(id as usize).map(|c| c.load(Ordering::Acquire)).unwrap_or(0)
    }

    pub fn set(&self, id: u64, word: u64) {
        let Some(cell) = self.cell(id as usize) else {
            return;
        };
        cell.store(word, Ordering::Release);
        self.high.fetch_max(id as usize + 1, Ordering::AcqRel);
    }

    /// Zero every touched cell and reset the `high` watermark. Called at a
    /// program boundary: the cells hold the finished program's PolyValue words,
    /// which wrap HandleTable slots the reset drains — leaving them for the next
    /// program means `mark_gcell_roots` scans STALE handle words (a recycled
    /// slot), which the sound GC can follow into corrupt entries. The store is
    /// reused (not dropped) so any spawned worker's `Arc` clone stays valid.
    pub fn reset(&self) {
        let high = self.high.load(Ordering::Acquire);
        for id in 0..high {
            if let Some(c) = self.cell(id) {
                c.store(0, Ordering::Release);
            }
        }
        self.high.store(0, Ordering::Release);
    }

    /// Every live cell word, for the GC root scan.
    pub fn words(&self) -> Vec<u64> {
        let high = self.high.load(Ordering::Acquire);
        (0..high).filter_map(|id| self.cell(id).map(|c| c.load(Ordering::Acquire))).collect()
    }
}

thread_local! {
    /// The store of the program THIS thread belongs to. A thread that never ran
    /// JS (a socket reader, a resolver) simply never touches it.
    static CURRENT: std::cell::RefCell<Option<Arc<GcellStore>>> = const {
        std::cell::RefCell::new(None)
    };
}

/// This thread's store, creating one if the thread is the first of its program.
fn current() -> Arc<GcellStore> {
    CURRENT.with(|c| {
        let mut slot = c.borrow_mut();
        slot.get_or_insert_with(|| Arc::new(GcellStore::new())).clone()
    })
}

/// A handle on a program's cells, to hand to a thread it spawns.
pub type GcellRef = Arc<GcellStore>;

/// The current program's store — what a spawning thread passes to its worker so
/// both SHARE the cells (where the old code snapshotted the values).
pub fn share() -> GcellRef {
    current()
}

/// Join `store`'s program: this thread now reads and writes ITS cells, so a
/// worker's write to a module global is visible to every other thread.
pub fn attach(store: GcellRef) {
    CURRENT.with(|c| *c.borrow_mut() = Some(store));
}

/// `GCELL_GET`/`GCELL_SET`'s backing.
pub fn get(id: u64) -> u64 {
    current().get(id)
}

pub fn set(id: u64, word: u64) {
    current().set(id, word);
}

/// Every cell word of this thread's program — the GC root scan reads it.
pub fn root_words() -> Vec<u64> {
    current().words()
}

/// Zero this thread's gcell store at a program boundary (see [`GcellStore::reset`]).
pub fn reset() {
    current().reset();
}
