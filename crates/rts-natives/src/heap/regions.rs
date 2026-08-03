//! Thread → shard affinity ("regions") — phase **T3** of
//! `the spec removed 2026-08-03 (see git history)`.
//!
//! A **region** is a contiguous, disjoint subset of the 32-shard space owned by
//! exactly one thread. A thread allocates only inside its own region.
//!
//! What this does NOT change, deliberately: the handle ENCODING. A handle still
//! carries its shard index in the low `SHARD_BITS`, so `shard_for_handle` and
//! every existing decode path keep working byte-for-byte. The only thing T3
//! moves is the *choice* of shard at allocation time. That is the whole point of
//! doing affinity before promotion (T4): it is a pure allocation-policy change,
//! so it can be A/B'd against today's behaviour without a second binary.
//!
//! Why it matters beyond locking: with one region per thread, the slot-table
//! base of `RTS_CLASS_IMPLEMENTATION.md` §4.1 stops being a `load
//! [SLOT_TABLES + shard*8]` and becomes an `iconst` — that document's C5, which
//! names this phase as its dependency.
//!
//! **Default OFF.** Set `RTS_REGIONS=1` to enable. Unset or `0` reproduces the
//! historical global round-robin exactly, same counter, same modulus.

use std::cell::Cell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::abi::handles::HANDLE_N_SHARDS as N_SHARDS;

/// `RTS_REGIONS=1` — allocate with deterministic thread→region affinity instead
/// of smearing each thread's objects over all 32 shards.
///
/// OFF by default on purpose. Affinity is a *policy* whose value is a
/// measurement (lock contention avoided vs. shard-space fragmentation when a
/// program is single-threaded), and every optimization in this area is being
/// validated by A/B on one binary. TODO(measure): no number is claimed here yet.
fn regions_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_REGIONS")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}

/// How many regions the shard space is cut into.
///
/// Derived, never a second magic number: it is the largest power of two that is
/// `<= N_SHARDS` and `>= available_parallelism()`. Two properties are load
/// bearing.
///
/// * **Power of two** — so `N_SHARDS / N_REGIONS` divides exactly. A remainder
///   would leave orphan shards that no thread ever allocates into, and those
///   shards would still be walked by every sweep: pure cost, zero benefit.
/// * **>= available_parallelism** — so a machine's worth of threads gets a
///   region each before any wrap-around (see [`assign_region`]). Clamped to
///   `N_SHARDS` because a region must own at least one shard; a machine with
///   more cores than shards simply shares.
///
/// The value is computed once and cached: `available_parallelism` is a syscall,
/// and — more importantly — region assignment must be STABLE for the life of the
/// process. Re-reading it per call could hand two threads overlapping ranges.
fn n_regions() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| {
        let par = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        // next_power_of_two(1) == 1, so a single-core / unknown machine yields
        // ONE region covering all 32 shards — i.e. exactly today's behaviour,
        // which is the right degenerate case.
        par.next_power_of_two().clamp(1, N_SHARDS)
    })
}

/// Shards per region. Exact by construction (see [`n_regions`]).
fn region_len() -> usize {
    N_SHARDS / n_regions()
}

/// Monotonic source of region ids. `fetch_add` and never reset: reuse of a
/// region id by a *later* thread is fine (the earlier thread is gone), but two
/// LIVE threads must never be handed the same id, and a monotonic counter is the
/// cheapest way to guarantee that for the first `n_regions()` threads.
static NEXT_REGION: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// This thread's region, assigned lazily on its first allocation and then
    /// never changed. Stability matters more than fairness here: a thread that
    /// migrated between regions would leave live objects behind in a region the
    /// local collector of T4 would later consider unowned.
    ///
    /// `None` until the first allocation — assigning at thread *creation* is not
    /// possible (there is no hook on `std::thread` spawn we control for every
    /// thread that reaches the heap, including tokio workers and the OS main).
    static MY_REGION: Cell<Option<usize>> = const { Cell::new(None) };

    /// Round-robin cursor. In region mode it is an offset INSIDE the region
    /// (`0..region_len()`); in the OFF path it is the historical absolute shard
    /// index (`0..N_SHARDS`). Keeping one cell for both keeps `alloc_entry`'s
    /// change to a single call.
    static CURSOR: Cell<usize> = const { Cell::new(0) };
}

/// The first shard of this thread's region, assigning the region if needed.
///
/// Assignment: `region = NEXT_REGION.fetch_add(1) % n_regions()`.
///
/// **When threads exceed regions this WRAPS**, and that is deliberate rather
/// than an oversight: thread `n_regions()` shares a region — and therefore the
/// shard `Mutex`es — with thread 0. The alternatives are worse. Refusing to
/// allocate is not an option; growing the shard space would change the handle
/// encoding (the shard index is 5 bits wide, `N_SHARDS` is a hard 32). So beyond
/// `n_regions()` live threads, contention returns to what it is today.
///
/// A future reader must know this, because it means T4's "one region, one
/// thread, local collection" invariant does NOT hold unconditionally: local
/// collection must either check the region's owner count or fall back to the
/// global cycle. T3 does not attempt to solve that.
fn region_base() -> usize {
    MY_REGION.with(|r| {
        if let Some(base) = r.get() {
            return base;
        }
        let idx = NEXT_REGION.fetch_add(1, Ordering::Relaxed) % n_regions();
        let base = idx * region_len();
        r.set(Some(base));
        base
    })
}

/// Pick the shard for the next allocation on the calling thread.
///
/// This is the ONLY behavioural hook T3 adds to the allocator: `alloc_entry`
/// calls it instead of computing `(v + 1) % N_SHARDS` itself.
///
/// Round-robin is kept *within* the region rather than pinning a thread to one
/// shard. Pinning would be better for locality but would concentrate every
/// object of a thread behind a single `Mutex`, so a second thread that ever
/// touches those handles (a `mark`, a `free`) serializes against the owner's
/// whole allocation stream. Spreading over the region keeps the existing
/// lock-splitting property while still bounding a thread to a known slice.
///
/// For a single-threaded program the effect is: it uses only its own region's
/// shards. Shard-lock contention is unaffected — there is none either way, since
/// contention needs two threads. GC sweep cost is likewise unaffected: the sweep
/// walks all `N_SHARDS` regardless of where entries live, and T3 does not touch
/// it (restricting the sweep to a region is T4).
pub(crate) fn next_shard() -> usize {
    if !regions_enabled() {
        // Historical path, preserved bit-for-bit: global round-robin over every
        // shard, per-thread cursor, post-increment.
        return CURSOR.with(|s| {
            let v = s.get();
            s.set((v + 1) % N_SHARDS);
            v
        });
    }

    let base = region_base();
    let len = region_len();
    CURSOR.with(|s| {
        let off = s.get() % len; // `%` guards against a cursor left over from a
        // pre-assignment allocation in the OFF path.
        s.set((off + 1) % len);
        base + off
    })
}

/// Test-only view of the knob, so sibling tests whose expectations are tied to
/// the DEFAULT allocation policy can opt out instead of asserting something the
/// other policy legitimately violates.
#[cfg(test)]
pub(crate) fn regions_enabled_for_test() -> bool {
    regions_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_partition_the_shard_space_exactly() {
        let len = region_len();
        assert_eq!(len * n_regions(), N_SHARDS, "regions must tile all shards");
        assert!(len >= 1);
    }

    #[test]
    fn a_thread_stays_inside_one_region() {
        // Only meaningful with the knob on; with it off this asserts the
        // historical behaviour instead (all shards reachable), so the test is
        // valid either way and needs no env mutation.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..(N_SHARDS * 4) {
            seen.insert(next_shard());
        }
        if regions_enabled() {
            assert_eq!(seen.len(), region_len());
            let base = region_base();
            assert!(seen.iter().all(|s| (base..base + region_len()).contains(s)));
        } else {
            assert_eq!(seen.len(), N_SHARDS);
        }
    }
}
