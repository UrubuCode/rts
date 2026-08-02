//! Live-heap accounting (handle count + payload bytes) and the GC-disable knob.
//!
//! Split out of `heap/handles.rs` (RTS_OPTIMIZATION.md §5 item **1.4**) for two
//! reasons: that file is an explicit draining target far over the 500-line
//! ceiling, and the accounting is now a small state machine rather than two
//! `fetch_add`s, so it deserves its own module with its own tests.
//!
//! ## What changed and why
//!
//! `alloc_entry` used to do `LIVE_BYTES.fetch_add` + `LIVE_HANDLES.fetch_add` on
//! **every** allocation: two read-modify-write operations on two process-global
//! cache lines that every thread writes, so every allocating thread bounces both
//! lines. The counters are only ever READ at a GC decision point, never per
//! allocation, so the RMW traffic buys nothing.
//!
//! The fix is the TLAB / .NET GC pattern: keep the delta **per shard**, and push
//! it into the global only once it is big enough to matter. The per-shard delta
//! lives inside [`crate::heap::handles::HandleTable`] itself — not in a side
//! table — so it sits on the cache line the allocator has already pulled in and
//! is mutated under the shard `Mutex` the allocation already holds. Cost per
//! allocation: two non-atomic integer adds. Cost per flush: two atomics, at most
//! once per [`HANDLE_BATCH`] allocations in that shard.
//!
//! Documented expectation: RTS_OPTIMIZATION.md §5 Tier 1 prices item 1.4 at
//! **~2.7% on `objbench`**. TODO(measure): no new number is claimed here.
//!
//! ## Staleness this introduces — bounded, never permanent
//!
//! Between flushes the globals under-report (or over-report, after frees) by at
//! most one batch per shard:
//!
//! * handles: `N_SHARDS * (HANDLE_BATCH - 1)` = **2 016**, against a
//!   [`GC_LIVE_FLOOR`](crate::heap::handles) of 500 000 — 0.4%.
//! * bytes: `N_SHARDS * BYTE_BATCH` = **8 MiB**, against a 64 MiB byte floor —
//!   12.5%.
//!
//! So the periodic GC can fire *late* by up to that much heap, which is exactly
//! the class of imprecision the floors already tolerate (they are order-of-
//! magnitude heuristics: see the `entry_heap_bytes` doc comment, which measures
//! only the large variants and reports 0 for the rest). What is NOT acceptable
//! is permanent drift, and there is none: every shard flushes within
//! `HANDLE_BATCH` of its own allocations, [`ShardLive::flush`] runs
//! unconditionally at the end of every sweep, and [`flush_all_shards`] makes the
//! numbers exact on demand for the readers that need exactness.
//!
//! ## Behaviour under `RTS_REGIONS`
//!
//! Unaffected either way, because the batch is per SHARD and the flush rate is
//! per allocation, not per shard-visit:
//!
//! * knob OFF (default): a thread round-robins all 32 shards, so each shard's
//!   delta reaches `HANDLE_BATCH` after `32 * HANDLE_BATCH` of that thread's
//!   allocations — but there are 32 shards, so globally still one flush per
//!   `HANDLE_BATCH` allocations.
//! * knob ON: a thread touches only its own region, so its shards fill (and
//!   flush) proportionally faster per shard and identically per allocation — and
//!   with affinity the flushed line is one a single thread owns, which is
//!   strictly better for the coherence traffic this item is about.
//!
//! The staleness bound above is `N_SHARDS`-wide in both modes, so it does not
//! depend on the knob at all.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Global live-slot count (allocs − frees). Only ever written by
/// [`ShardLive::flush`]; read by the GC trigger and by `gc.live_count()`.
///
/// Kept as the safety cap's counter too: pathological programs that allocate in
/// a loop without ever collecting used to leak silently, and [`HANDLES_MAX`]
/// turns that into a loud diagnostic.
pub(crate) static LIVE_HANDLES: AtomicUsize = AtomicUsize::new(0);

/// Global live payload bytes — the size half of [`LIVE_HANDLES`], with the same
/// single writer.
pub(crate) static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Hard cap on simultaneously-live handles. Each handle costs from tens of bytes
/// (short string) to megabytes (large buffer); 5M short strings is already ~5 GB,
/// so the cap converts an unbounded leak into a clear abort instead of an OS OOM.
///
/// With batched flushing the cap is now detected up to `N_SHARDS * HANDLE_BATCH`
/// (2 048) allocations late — 0.04% of the cap, and the cap is itself a
/// safety net two orders of magnitude above any real workload.
const HANDLES_MAX: usize = 5_000_000;

/// Per-shard handle delta at which a flush is forced.
///
/// 64 is chosen from the two bounds it controls: it divides the per-allocation
/// atomic traffic by 64, and it keeps the worst-case global staleness
/// (`32 * 63` handles) three orders of magnitude below `GC_LIVE_FLOOR`.
/// TODO(measure): the constant has not been swept; 64 is a reasoned default, not
/// a measured optimum.
const HANDLE_BATCH: isize = 64;

/// Per-shard byte delta at which a flush is forced, INDEPENDENTLY of
/// [`HANDLE_BATCH`].
///
/// A byte trigger is required because one handle can carry an arbitrarily large
/// payload: 64 allocations of a 1 MB buffer each is 64 MB of drift under a
/// handle-only rule, which would let the 64 MB byte floor be missed entirely.
/// 256 KiB caps the worst case at 8 MiB across all shards.
const BYTE_BATCH: isize = 256 * 1024;

/// The per-shard half of the live accounting. Lives inside the shard's
/// `HandleTable` (see the module docs) and is therefore always mutated under
/// that shard's `Mutex` — hence plain integers, no atomics.
///
/// Signed on purpose: a shard's delta goes negative whenever it frees more than
/// it allocated since the last flush (a sweep does exactly that).
#[derive(Debug, Default)]
pub(crate) struct ShardLive {
    handles: isize,
    bytes: isize,
}

impl ShardLive {
    /// Account one new live slot of `bytes` payload.
    #[inline]
    pub(crate) fn on_alloc(&mut self, bytes: usize) {
        self.handles += 1;
        self.bytes += bytes as isize;
        self.maybe_flush();
    }

    /// Account one slot going away, with the payload measured at free time.
    ///
    /// Free is batched through exactly the same path as alloc, deliberately: an
    /// increment path that batches and a decrement path that does not would make
    /// the two halves diverge in opposite directions and turn a bounded lag into
    /// an unbounded one.
    #[inline]
    pub(crate) fn on_free(&mut self, bytes: usize) {
        self.handles -= 1;
        self.bytes -= bytes as isize;
        self.maybe_flush();
    }

    /// Account a whole sweep at once: `handles` slots and `bytes` payload gone.
    /// Bulk form of [`Self::on_free`] — the sweep caller flushes explicitly
    /// afterwards, so this deliberately does not consult the batch thresholds.
    pub(crate) fn on_sweep(&mut self, handles: usize, bytes: usize) {
        self.handles -= handles as isize;
        self.bytes -= bytes as isize;
    }

    #[inline]
    fn maybe_flush(&mut self) {
        if self.handles.abs() >= HANDLE_BATCH || self.bytes.abs() >= BYTE_BATCH {
            self.flush();
        }
    }

    /// Push this shard's accumulated delta into the globals. Idempotent (the
    /// delta is zeroed), and cheap enough to call unconditionally at the end of a
    /// sweep — which is what keeps the post-collection numbers exact.
    pub(crate) fn flush(&mut self) {
        let handles = std::mem::take(&mut self.handles);
        let bytes = std::mem::take(&mut self.bytes);
        if handles != 0 {
            let total = add_saturating(&LIVE_HANDLES, handles);
            // Cap check moved here from the allocation site: it now costs
            // nothing on the fast path and still fires within one batch of the
            // real limit.
            if total >= HANDLES_MAX {
                eprintln!(
                    "RTS runtime: handle table exceeded limit of {HANDLES_MAX} live handles; aborting (likely string/array allocations in unbounded loop without GC — codegen does not emit auto-free yet)"
                );
                std::process::abort();
            }
        }
        if bytes != 0 {
            // Saturating, never wrapping: an `Entry::Vec` can GROW in place after
            // it was allocated, so a free can subtract more than the matching
            // alloc added. A wrap to ~2^64 would read as "byte floor permanently
            // exceeded" and run the GC every 256 allocations for the rest of the
            // process. Saturating keeps the counter an UNDERestimate, which only
            // ever makes the trigger later — never wrong.
            add_saturating(&LIVE_BYTES, bytes);
        }
    }
}

/// Apply a signed delta to an unsigned global without ever wrapping.
fn add_saturating(counter: &AtomicUsize, delta: isize) -> usize {
    if delta >= 0 {
        counter.fetch_add(delta as usize, Ordering::Relaxed) + delta as usize
    } else {
        let sub = delta.unsigned_abs();
        counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(sub))
            })
            .map(|prev| prev.saturating_sub(sub))
            .unwrap_or(0)
    }
}

/// Current live handle count. A plain load — deliberately NOT a flush, because
/// this is read on the GC tick path every `GC_TICK_INTERVAL` allocations and
/// locking all 32 shards there would cost more than the atomics this item
/// removes. Callers that need an exact value call [`flush_all_shards`] first.
pub fn live_handle_count() -> usize {
    LIVE_HANDLES.load(Ordering::Relaxed)
}

/// Current live payload bytes. Same staleness contract as [`live_handle_count`].
pub fn live_byte_count() -> usize {
    LIVE_BYTES.load(Ordering::Relaxed)
}

/// Drain every shard's pending delta into the globals, making both counters
/// exact as of this instant.
///
/// Locks each shard in turn and holds no other lock while doing so, so it cannot
/// deadlock against the allocator. Call it before READING the counters for
/// anything a user or a test can observe (`gc.collect()`'s freed-count, the
/// `gc.live_count()` ABI) — not on the allocation path.
pub fn flush_all_shards() {
    for shard in crate::heap::handles::shards() {
        shard.lock().unwrap_or_else(|e| e.into_inner()).flush_live();
    }
}

/// `RTS_GC_DISABLE=1` turns off the periodic collector. Useful when diagnosing a
/// sweep that frees reachable handles (stack-scan / safepoint coverage bug).
///
/// Cached in a `OnceLock` — this is consulted from the allocation path's GC-tick
/// branch, and `std::env::var` locks the process environment and allocates a
/// `String` on every call. Same pattern as
/// `rts-codegen-new/src/front/run/clifflags.rs`. Reading the knob once means a
/// mid-process `set_var` no longer takes effect, which is correct: the collector
/// being armed or not must not change under a running program.
pub(crate) fn gc_disabled() -> bool {
    static OFF: OnceLock<bool> = OnceLock::new();
    *OFF.get_or_init(|| {
        std::env::var("RTS_GC_DISABLE")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batching_is_exact_after_flush() {
        let mut s = ShardLive::default();
        let before = live_handle_count();
        for _ in 0..(HANDLE_BATCH - 1) {
            s.on_alloc(0);
        }
        // Still pending: nothing crossed the batch threshold.
        assert_eq!(s.handles, HANDLE_BATCH - 1);
        s.flush();
        assert_eq!(s.handles, 0);
        assert!(live_handle_count() >= before + (HANDLE_BATCH as usize - 1));
        // Give the global back what this test added, so parallel tests reading
        // the shared counter see a net-zero delta.
        for _ in 0..(HANDLE_BATCH - 1) {
            s.on_free(0);
        }
        s.flush();
    }

    #[test]
    fn crossing_the_batch_flushes_by_itself() {
        let mut s = ShardLive::default();
        for _ in 0..HANDLE_BATCH {
            s.on_alloc(0);
        }
        assert_eq!(s.handles, 0, "batch boundary must auto-flush");
        for _ in 0..HANDLE_BATCH {
            s.on_free(0);
        }
        s.flush();
    }

    #[test]
    fn a_single_large_payload_flushes_on_the_byte_rule() {
        let mut s = ShardLive::default();
        s.on_alloc(BYTE_BATCH as usize);
        assert_eq!(s.bytes, 0, "byte threshold must auto-flush");
        assert_eq!(s.handles, 0, "flush drains both halves together");
        s.on_free(BYTE_BATCH as usize);
        s.flush();
    }

    #[test]
    fn negative_delta_never_wraps() {
        let c = AtomicUsize::new(3);
        assert_eq!(add_saturating(&c, -10), 0);
        assert_eq!(c.load(Ordering::Relaxed), 0);
    }
}
