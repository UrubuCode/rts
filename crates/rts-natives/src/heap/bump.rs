//! Per-thread allocation fast path for object payload storage —
//! RTS_OPTIMIZATION.md §5 Tier **4.2**.
//!
//! **Default OFF.** `RTS_BUMP=1` enables it. With the knob unset this module is
//! two thin wrappers that do exactly what the call sites did before
//! (`Box::new(Vec::with_capacity(n))` on the way in, `drop` on the way out), so
//! the OFF path is byte-for-byte today's behaviour and the ON path is A/B-able
//! against it in one binary — same discipline as [`crate::heap::regions`].
//!
//! ## What 4.2 asked for, and what is actually reachable here
//!
//! 4.2 asks for a bump-pointer nursery. Two findings from reading the tree bound
//! what that can mean today, and both are recorded here because they are the
//! reason this module has the shape it has rather than the shape the item's
//! title suggests.
//!
//! ### Finding 1 — the shard `Mutex` is NOT where the cost is
//!
//! `crates/rts-engine/examples/rtse_probe.rs` prices allocation at **40.4×** and
//! an uncontended mutex at **2.4×**; `rts-value-probe`'s kernel W prices today's
//! whole path at **347 ns** against **5.84 ns** for a block allocation without
//! the lock. The 341 ns delta is dominated by the general-purpose `malloc` for
//! the payload, not by the lock around the slot table. An allocator change that
//! removed only the lock would be aiming at the small term.
//!
//! ### Finding 2 — a LOCK-FREE bump into the slot table is unsound today
//!
//! RTS_OPTIMIZATION.md §4.3 (which refutes its own earlier draft, citing
//! `collector/scan.rs:183-196`) establishes that **this collector has no
//! stop-the-world phase**: `mark_handle` and `sweep_all_shards` run with every
//! other thread live, and the shard `Mutex` is what serializes a mutator against
//! a concurrent `sweep_unmarked` reclaiming the same slot. A thread publishing a
//! `Slot` outside that lock would race the sweep on a partially-written `Entry`,
//! and a slot reserved-but-not-yet-published would be indistinguishable from
//! garbage to a sweep that reached it first. Making the publish lock-free needs
//! an epoch / safepoint scheme first (§4.3 names it), which is a separate item.
//!
//! ### What follows
//!
//! Bump allocation is **not** blocked on a moving collector — §4.1 refutes that
//! coupling explicitly, and the slot index never moves in any case. It is blocked
//! on the *object representation*: an object is `Entry::Vec(Box<Vec<i64>>)`
//! (`heap/shapes/words.rs`, `heap/payload_ops.rs::vec_new_object_shaped`), and a
//! `Vec` owns its buffer through the global allocator. **No bump arena can back a
//! `Vec`** — not without the stable-`Allocator` API, and not without changing a
//! representation named at 386 sites. The genuine bump pointer of kernel W's
//! `moving_slab`/`region_slab` rows arrives with the inline-slot object layout of
//! `RTS_CLASS_IMPLEMENTATION.md`, and it arrives there, not here.
//!
//! So this module implements the part of 4.2 that IS reachable behind the current
//! representation and captures the dominant (40.4×) term: a **thread-local,
//! size-classed recycler for the payload buffers themselves**. A dead object's
//! buffer goes back to the thread's pool instead of to `free()`; the next object
//! of a comparable size takes it back instead of calling `malloc`. It is a free
//! list, not a bump pointer, and this comment exists so nobody later reads the
//! module name and believes otherwise.
//!
//! TODO(measure): no number is claimed for this module. The numbers above are
//! attributed to the probes that produced them and describe the path, not this
//! change.
//!
//! ## GC interaction — nothing to sweep
//!
//! This is the property that makes the recycler safe without touching the
//! collector at all, and it is worth stating precisely because requirement 3 of
//! the item is exactly this question.
//!
//! * **Handles are untouched.** A handle is still `gen | slot | shard`, still
//!   produced by `alloc_entry`, still decoded by `shard_for_handle`. This module
//!   never sees a handle and never hands out an address. The invariant
//!   `rts-threading-model.md` calls load-bearing — "payload = slot index, never a
//!   pointer" — is not in contact with this code.
//! * **A pooled buffer is not a heap object.** It enters the pool only from
//!   [`recycle`], which is called at the two points an entry ALREADY dies today
//!   (`HandleTable::free` and `HandleTable::sweep_unmarked`, immediately after
//!   `cleanup_entry` and before `slot.entry = Entry::Free`). Once pooled it is
//!   owned by no `Entry`, so `mark` cannot reach it and `sweep_unmarked` has
//!   nothing to visit. There is no third state: a buffer is either inside a live
//!   `Entry` (swept exactly as today) or inside the pool (dead, untraced).
//! * **It cannot cause false retention.** The conservative scanner walks the
//!   STACK, the gcells and the pinned roots — never a heap buffer. Stale handle
//!   words left in a pooled buffer's spare capacity are therefore invisible to it,
//!   and [`recycle`] `clear()`s the buffer regardless.
//! * **It cannot cause premature free.** Nothing is reclaimed earlier than it is
//!   today; the recycler only changes where the memory of an already-dead entry
//!   goes.
//! * **Live accounting is unchanged.** `entry_heap_bytes` measures `Vec::len`,
//!   and both `on_alloc` and `on_free` see exactly the lengths they see today.
//!   The one honest caveat is that pooled *capacity* is memory `LIVE_BYTES` stops
//!   counting, so the pool is hard-bounded — see [`MAX_PER_CLASS`].
//!
//! ## Regions interaction
//!
//! Orthogonal, in both knob positions. `heap::regions` chooses WHICH SHARD a slot
//! comes from; this chooses where the PAYLOAD BUFFER comes from. The pool is
//! thread-local, so with `RTS_REGIONS=1` (one thread per region) a thread's pool
//! and its region are the same ownership domain and recycling stays inside it;
//! with the knob off (global round-robin) the pool is still per-thread and still
//! correct — buffers have no shard affinity to violate, because a buffer is not
//! addressed by handle bits.
//!
//! The one cross-thread flow is deliberate: a sweep running on thread B can
//! recycle a buffer allocated by thread A, and it lands in B's pool. That is
//! sound (`Vec<i64>` is `Send`, and the entry is dead), and it is the behaviour
//! that keeps a producer/consumer program from starving one pool while the other
//! overflows.

use std::cell::RefCell;
use std::sync::OnceLock;

use crate::heap::handles::Entry;

/// `RTS_BUMP=1` — recycle object payload buffers per thread instead of returning
/// them to the global allocator.
///
/// OFF by default, like every allocator knob in this crate: this changes the
/// memory profile (pooled capacity is retained, see [`MAX_PER_CLASS`]) as well as
/// the speed profile, and both halves have to be measured on the same binary
/// before it becomes the default.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_BUMP")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}

/// Pooled capacities, in `i64` words. One class per row of [`POOLS`].
///
/// Chosen from the object sizes the representation actually produces: a shaped
/// object is `1 + field_count` words (`payload_ops::vec_new_object_shaped`), so
/// classes of 4/8/16/32 words cover a 3-, 7-, 15- and 31-field class plus the
/// shape header. Anything wider is a collection, not an instance, and its
/// lifetime is not the allocation-per-iteration shape this item is about — those
/// fall through to the global allocator unchanged.
const CLASS_WORDS: [usize; 4] = [4, 8, 16, 32];

/// Buffers retained per class per thread.
///
/// This is the ONLY bound on pooled memory, and it is deliberately a count rather
/// than a byte budget so the worst case is a constant a reader can evaluate here:
/// `64 * (4 + 8 + 16 + 32) * 8 B` = **30 KiB per thread**, all of it capacity
/// `LIVE_BYTES` no longer counts (see the module docs). 30 KiB is below the
/// noise floor of a 64 MiB byte floor, which is what makes a count sufficient.
///
/// TODO(measure): 64 is a reasoned bound, not a swept optimum.
const MAX_PER_CLASS: usize = 64;

thread_local! {
    /// This thread's free buffers, one bucket per [`CLASS_WORDS`] entry.
    ///
    /// `RefCell` rather than `UnsafeCell`: the borrow is never held across a call
    /// that could re-enter (see [`with_pools`]), so the check is a predictable
    /// branch, and getting this wrong in an allocator is not a class of bug worth
    /// trading a branch for.
    static POOLS: RefCell<[Vec<Box<Vec<i64>>>; CLASS_WORDS.len()]> =
        RefCell::new(std::array::from_fn(|_| Vec::with_capacity(MAX_PER_CLASS)));
}

/// Run `f` against this thread's pools, or return `None` when they are not
/// reachable.
///
/// Two ways they are not: TLS teardown has already destroyed them (`try_with`),
/// or a borrow is somehow live (`try_borrow_mut`). Both fall back to the global
/// allocator instead of panicking — an allocator that aborts a program because
/// its cache is unavailable is worse than one that is briefly slower.
///
/// Reentrancy: the closures passed below only push/pop a `Box` and call
/// `Vec::clear`. Neither touches the HandleTable, so nothing here can re-enter a
/// shard `Mutex` — which matters because [`recycle`] is called with one held.
#[inline]
fn with_pools<R>(f: impl FnOnce(&mut [Vec<Box<Vec<i64>>>; CLASS_WORDS.len()]) -> R) -> Option<R> {
    POOLS
        .try_with(|p| p.try_borrow_mut().ok().map(|mut g| f(&mut g)))
        .ok()
        .flatten()
}

/// Index of the smallest class that can hold `words`, or `None` when `words`
/// exceeds every class.
#[inline]
fn class_for_request(words: usize) -> Option<usize> {
    CLASS_WORDS.iter().position(|&c| c >= words)
}

/// Index of the largest class a buffer of `cap` words fully satisfies, or `None`
/// when it is too small to serve even the smallest class.
///
/// Deliberately floors instead of rounding: a buffer filed under class `i` must
/// be able to satisfy every [`acquire`] for that class WITHOUT reallocating, and
/// that is only guaranteed when `cap >= CLASS_WORDS[i]`. A 12-word buffer files
/// under the 8-word class and keeps its extra 4 words — slack, never a promise
/// broken.
#[inline]
fn class_for_buffer(cap: usize) -> Option<usize> {
    CLASS_WORDS.iter().rposition(|&c| c <= cap)
}

/// An EMPTY payload buffer with capacity for at least `words` `i64`s.
///
/// The replacement for `Box::new(Vec::with_capacity(words))` on the object
/// construction path. With the knob off it IS that expression.
///
/// The returned buffer has `len() == 0` and unspecified contents beyond it, which
/// is exactly the contract `Vec::with_capacity` gives — every caller fills it by
/// `push`/`resize` before publishing it into an `Entry`.
#[inline]
pub fn acquire(words: usize) -> Box<Vec<i64>> {
    if enabled() {
        if let Some(class) = class_for_request(words) {
            if let Some(Some(buf)) = with_pools(|pools| pools[class].pop()) {
                debug_assert!(buf.is_empty(), "pooled buffer must be cleared on release");
                debug_assert!(buf.capacity() >= words);
                return buf;
            }
            // Miss: allocate at the CLASS size, not the requested size, so the
            // buffer comes back into the pool wide enough to serve the whole
            // class. Allocating exactly `words` would file a 5-word buffer under
            // the 4-word class and lose the 8-word slot forever.
            return Box::new(Vec::with_capacity(CLASS_WORDS[class]));
        }
    }
    Box::new(Vec::with_capacity(words))
}

/// Return a dying entry's payload to this thread's pool, then drop whatever is
/// left of it.
///
/// Called at the two points an entry already dies — `HandleTable::free` and
/// `HandleTable::sweep_unmarked` — taking ownership of the `Entry` that was about
/// to be overwritten with `Entry::Free`. Taking the whole `Entry` rather than a
/// `&mut` is what makes this allocation-free: `std::mem::replace(e, Entry::Free)`
/// at the call site costs nothing, whereas `mem::take` on the `Box<Vec<i64>>`
/// inside would allocate a fresh `Box` for the placeholder and give back exactly
/// what this module is trying to save.
///
/// **Runs under the shard `Mutex`** (both call sites hold it). Everything it does
/// is a thread-local push or a `drop` — the same `free()` that happens under that
/// lock today — so it adds no re-entrancy on the allocation path.
#[inline]
pub fn recycle(entry: Entry) {
    if !enabled() {
        return; // `entry` drops here — today's behaviour exactly.
    }
    let Entry::Vec(mut buf) = entry else {
        return;
    };
    let Some(class) = class_for_buffer(buf.capacity()) else {
        return;
    };
    // Clearing before pooling drops the length to 0 and leaves the stale words in
    // spare capacity. They are unreachable (see the module docs on false
    // retention) and every reuse overwrites what it reads.
    buf.clear();
    // `with_pools` returning the buffer means it was NOT pooled — either the
    // bucket is full or TLS is gone — and it drops on the next line.
    let _rejected = with_pools(|pools| {
        let bucket = &mut pools[class];
        if bucket.len() < MAX_PER_CLASS {
            bucket.push(buf);
            None
        } else {
            Some(buf)
        }
    });
}

/// Drop every pooled buffer on this thread.
///
/// Not called on any hot path. It exists for two callers that will want it: a
/// future `gc.collect()` that wants the process's RSS to reflect a full
/// collection, and the tests below, which must not leave a populated pool behind
/// for a sibling test running on the same thread.
pub fn drain_thread_pool() {
    let _ = with_pools(|pools| {
        for bucket in pools.iter_mut() {
            bucket.clear();
        }
    });
}

/// Buffers currently pooled on this thread, per class. Test/diagnostic only.
#[cfg(test)]
fn pooled_counts() -> [usize; CLASS_WORDS.len()] {
    with_pools(|pools| std::array::from_fn(|i| pools[i].len())).unwrap_or([0; CLASS_WORDS.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_are_ascending_and_cover_the_instance_sizes() {
        assert!(CLASS_WORDS.windows(2).all(|w| w[0] < w[1]));
        // A 2-field class is `1 + 2` words and must land in the smallest class.
        assert_eq!(class_for_request(3), Some(0));
        assert_eq!(class_for_request(4), Some(0));
        assert_eq!(class_for_request(5), Some(1));
        assert_eq!(class_for_request(33), None, "wide payloads are not pooled");
    }

    #[test]
    fn a_buffer_files_under_the_class_it_fully_satisfies() {
        // Floors, never rounds up: 12 words cannot serve the 16-word class.
        assert_eq!(class_for_buffer(12), Some(1));
        assert_eq!(class_for_buffer(16), Some(2));
        assert_eq!(class_for_buffer(3), None);
    }

    #[test]
    fn acquire_always_satisfies_the_request() {
        // Valid with the knob in EITHER position — the capacity contract is the
        // one thing that must not depend on it.
        for n in [0usize, 1, 3, 4, 5, 31, 32, 33, 4096] {
            let b = acquire(n);
            assert!(b.is_empty());
            assert!(b.capacity() >= n, "acquire({n}) gave {}", b.capacity());
        }
    }

    #[test]
    fn recycling_is_a_no_op_with_the_knob_off() {
        if enabled() {
            return; // The ON-path behaviour is asserted by the test below.
        }
        drain_thread_pool();
        recycle(Entry::Vec(Box::new(Vec::with_capacity(8))));
        assert_eq!(pooled_counts(), [0; CLASS_WORDS.len()]);
    }

    #[test]
    fn a_recycled_buffer_comes_back_cleared_and_wide_enough() {
        if !enabled() {
            return; // Requires RTS_BUMP=1; the OFF path is covered above.
        }
        drain_thread_pool();
        let mut v = Box::new(Vec::with_capacity(8));
        v.extend_from_slice(&[1, 2, 3]);
        let addr = v.as_ptr();
        recycle(Entry::Vec(v));
        assert_eq!(pooled_counts()[1], 1);

        let back = acquire(5);
        assert!(back.is_empty(), "a pooled buffer must come back cleared");
        assert!(back.capacity() >= 8);
        assert_eq!(back.as_ptr(), addr, "the SAME allocation must be reused");
        drop(back);
        drain_thread_pool();
    }

    #[test]
    fn a_full_bucket_drops_instead_of_growing_without_bound() {
        if !enabled() {
            return;
        }
        drain_thread_pool();
        for _ in 0..(MAX_PER_CLASS * 3) {
            recycle(Entry::Vec(Box::new(Vec::with_capacity(4))));
        }
        assert_eq!(pooled_counts()[0], MAX_PER_CLASS);
        drain_thread_pool();
    }

    #[test]
    fn a_non_vec_entry_is_ignored() {
        // `recycle` consumes every dying entry, not only the pooled variants; the
        // rest must simply drop, exactly as they do today.
        recycle(Entry::Free);
        recycle(Entry::BooleanBox(true));
    }
}
