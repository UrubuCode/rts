//! Chunked, append-only, **stable-address** slot storage —
//! `RTS_OPTIMIZATION.md` §5 Tier **3.1**.
//!
//! **Default OFF.** `RTS_SLAB=1` enables it. With the knob unset,
//! [`store::SlotStore`] is a `Vec<Slot>` and the behaviour is byte-for-byte
//! what `HandleTable` did before this module existed, so the two paths are
//! A/B-able on one binary — the same discipline as [`crate::heap::regions`]
//! (`RTS_REGIONS=1`) and [`crate::heap::bump`] (`RTS_BUMP=1`).
//!
//! ## The problem this solves, stated precisely
//!
//! `HandleTable::slots` is a `Vec<Slot>`. A `Vec` **reallocates**: the moment it
//! outgrows its capacity every address into it becomes dangling. That is why
//! `payload_ops.rs` can write, deliberately, *"no pointer survives the call
//! boundary in either direction"* — there is no address in this table that is
//! valid for longer than one locked call.
//!
//! Tier 3.2 wants the opposite: it wants the emitted code to compute a slot's
//! address in pure Cranelift IR and `load` from it, hoisted out of loops
//! (`RTS_CLASS_IMPLEMENTATION.md` §4.1). That is impossible on storage that
//! moves. **3.1 is the precondition, and this module is 3.1.**
//!
//! ## The storage
//!
//! Per shard, slots live in fixed-size **chunks** of [`chunks::CHUNK_SLOTS`]
//! slots. A chunk is allocated once, fully initialized, then published into a
//! process-global flat table by an atomic pointer store — and it is **never
//! moved, never reallocated and never freed**. Only the chunk *list* grows, and
//! it grows by filling in one previously-null entry of a fixed-size table, so
//! the table itself never moves either.
//!
//! Consequently: **the address a given slot index resolves to is fixed for the
//! lifetime of the process**, not merely for the lifetime of the handle that
//! names it. That is a stronger guarantee than 3.1 asks for and it is free — it
//! falls out of "never free a chunk".
//!
//! ## How a handle resolves to an address
//!
//! Unchanged handle contract: `gen | slot | shard`, exactly the layout
//! `crate::abi::handles` defines, and `shard_for_handle` still decodes it.
//! Given the 48-bit payload:
//!
//! ```text
//! shard = payload & SHARD_MASK                       ; 5 bits
//! idx   = payload >> SHARD_BITS                      ; per-shard slot index
//! chunk = idx >> CHUNK_BITS
//! off   = idx & CHUNK_MASK
//! base  = load [CHUNK_TABLE + (shard*MAX_CHUNKS + chunk)*8]   ; ONE flat load
//! addr  = base + off * SLOT_STRIDE
//! ```
//!
//! The two-level index (`shard`, then `chunk`) collapses into **one** table load
//! because the table is flat — `table[shard * MAX_CHUNKS + chunk]`. This is
//! exactly the shape `rts-value-probe`'s kernel row **H7** measured, and H7 is
//! why chunking is priced at parity with the flat slab (1.54 vs H4's 1.51 ns):
//! the cost is a shift and a mask, not a second dependent load.
//!
//! A reader that already holds a **valid** handle needs no synchronization to
//! compute that address, because nothing it reads on the way can change: the
//! chunk table entry is written once (`Release`) and read `Acquire`, and the
//! chunk it points at never moves.
//!
//! ## What 3.1 does NOT deliver — read this before building on it
//!
//! Computing a stable *address* is not the same as being allowed to *read the
//! `Entry` at it* without the shard `Mutex`. An `Entry` is an enum holding
//! `Box`/`Vec` payloads that `free`, `sweep_unmarked` and `cleanup_entry` mutate
//! under the lock, and `RTS_OPTIMIZATION.md` §4.3 established that this
//! collector has **no stop-the-world phase** — the sweep runs with every mutator
//! live. So after this change:
//!
//! * A lock-free reader may read the slot generation (it is an
//!   `AtomicU16`, see the ABA section) and may compute an address.
//! * A lock-free reader may **not** read the `Entry`. Doing so races the sweep.
//!
//! The lock-free field read Tier 3.2 wants therefore needs *both* this module
//! *and* the inline-slot block layout of `RTS_CLASS_IMPLEMENTATION.md` §4.2 —
//! plain `i64` field words living in the slot itself rather than behind
//! `Entry::Vec(Box<Vec<i64>>)`. Those words are immune to the sweep's `Box`
//! churn in a way an `Entry` is not. That layout also needs the object/array
//! representation split (§8.4), which no measurement in either document covers.
//! **3.1 does not attempt it and this module does not pretend to.**
//!
//! ## The ABA hazard, answered deliberately
//!
//! §5 3.1 names it: *"if a slot's memory is reused without waiting for readers,
//! a suspended thread holding a stale handle can land on a repurposed slot"*.
//!
//! Here the memory **is** reused — that is the point of `free_list`, and a
//! chunked slab makes reuse more visible because the address is now genuinely
//! stable, so a stale handle lands on a *live, correctly-typed, wrong* slot
//! rather than on freed memory. Classic ABA.
//!
//! The answer is the generation counter, and these are the properties it needs
//! for that answer to be sound rather than incidental:
//!
//! 1. **Every reuse bumps it.** `alloc_in_shard` calls `next_generation` on the
//!    `free_list` path. Pre-existing, unchanged.
//! 2. **A reader validates before trusting the slot.** Every accessor already
//!    compares `slot.generation() != expected_gen`. Pre-existing, unchanged.
//! 3. **The read of the generation is a well-defined atomic read, not a data
//!    race.** This is what 3.1 adds: the field is now an `AtomicU16` read
//!    `Relaxed`. Under the old `Mutex<Vec<Slot>>` every read was under the lock,
//!    so a plain `u16` was fine; the whole purpose of stable addresses is to
//!    permit a read that is *not* under the lock, and a plain `u16` read
//!    concurrently with `set_generation` is UB in Rust and a torn read in
//!    principle. `Relaxed` compiles to the same `mov`, so this costs nothing and
//!    makes the guarantee real.
//! 4. **`Relaxed` is the correct ordering, and here is why.** The generation is
//!    used as a *rejection* test, never to publish data. A reader that loads a
//!    stale generation and rejects is correct. A reader that loads the current
//!    generation is, by definition, looking at the slot it means to look at. It
//!    never uses the generation to acquire a payload written by another thread —
//!    payload access is under the shard `Mutex`, which supplies its own
//!    acquire/release. If 3.2 later reads *payload words* lock-free, that read
//!    needs its own release/acquire pairing with the writer; the generation
//!    check does not provide it, and assuming it does would be the bug.
//!
//! **The residual hole, stated rather than hidden:** the generation is 16 bits
//! and wraps. A stale handle that survives exactly `0xFFF7` reuses of its slot
//! validates against a slot it does not own. This hole exists *today*, in the
//! `Vec` path, with identical arithmetic — 3.1 neither introduces nor closes it.
//! Closing it needs either a wider generation (no free bits in the handle) or
//! reclamation deferral (epoch/hazard pointers), which §4.3 names and which is
//! not this item.
//!
//! ## GC: sweep and slot reuse
//!
//! `collector/cycle.rs::finish_cycle` marks, then `sweep_all_shards`. Neither
//! changes behaviour, and neither needs to know this module exists:
//!
//! * **Sweep** walks slot indices `0..len` of the shard. On the `Vec` path that
//!   is one contiguous iteration; on the chunked path it is a nested walk over
//!   published chunks, visiting the same indices in the same order, under the
//!   same shard `Mutex`. The iteration order is identical, so the freeing order
//!   is identical.
//! * **Reuse** is unchanged: a swept slot's index is pushed onto `free_list` and
//!   handed back by a later `alloc_in_shard`, which bumps the generation. The
//!   slot's *memory* is reused in place — which is exactly what makes the address
//!   stable and what makes the ABA argument above load-bearing.
//! * **Chunks are never reclaimed.** A shard that peaked at N slots keeps the
//!   chunks for N slots forever. This is a deliberate memory/stability trade: a
//!   freed chunk is a dangling address, and 3.1's entire value is that no address
//!   dangles. The bound is [`chunks::MAX_CHUNKS`] per shard, and it sits far
//!   above the `HANDLES_MAX` abort cap (see that constant), so exhaustion is
//!   unreachable before the existing safety cap fires.
//! * **The conservative scanner is unaffected.** It walks stack words, gcells and
//!   pinned roots, decodes `gen | slot | shard` and validates against the live
//!   table through `mark_handle` → `shard_for_handle` → `mark`. Every one of
//!   those steps is byte-for-byte unchanged. No address produced by this module
//!   is ever placed in a scanned location, so the scanner cannot see one and
//!   cannot mistake one for a handle.
//! * **Non-moving by construction.** Chunks do not move, so there is nothing for
//!   a non-moving collector to disagree with. When the copying nursery of
//!   `gc-generational-design.md` arrives it moves *blocks*, repointing one word
//!   per object; the slot itself — the thing this module stabilizes — is exactly
//!   the indirection that makes that free (kernel H row H10, 1.50 ns, at parity
//!   with the immovable H4's 1.51).
//!
//! ## Lock discipline — the growth path must not re-enter
//!
//! `std`'s `Mutex` is not reentrant, and `alloc_entry` can fire a whole GC cycle
//! that walks every shard. Fifteen sites were fixed for allocating or re-locking
//! inside a `with_entry`/`with_rtse` closure. The growth path here is built not
//! to add a sixteenth:
//!
//! * Chunk allocation runs **inside** `alloc_in_shard`, i.e. already holding this
//!   shard's `Mutex`. It calls the global allocator (`Box<[Slot]>`) and nothing
//!   else. It never calls `alloc_entry`, never takes another shard's lock, never
//!   re-takes its own, and never runs a GC cycle — the tick check happens in
//!   `alloc_entry` *before* the lock is taken and is untouched.
//! * Publishing a chunk is a single `AtomicPtr::store(Release)`. No lock, no
//!   allocation, no callback.
//! * The chunk table is a `static` array of `AtomicPtr`, zero-initialized in BSS.
//!   It has no lazy init, so no `OnceLock` re-entrancy is possible on the path
//!   the allocator takes.
//!
//! ## Interaction with the two things that landed today
//!
//! * **[`crate::heap::regions`] (`RTS_REGIONS=1`) — composes, and 3.1 makes it
//!   worth more.** Regions choose *which shard* a slot comes from; this chooses
//!   *how that shard stores its slots*. Orthogonal in all four knob
//!   combinations. The interesting composition is forward-looking: with one
//!   region per thread the `shard` component of the address computation is a
//!   compile-time constant, so `load [CHUNK_TABLE + (shard*MAX_CHUNKS + chunk)*8]`
//!   loses its `shard*` term — the H9/H11 rows (0.99 vs 1.50 ns) are exactly that
//!   saving, and `regions.rs`'s own header already names §4.1 as the reason it
//!   exists. One caveat inherited from that module: when live threads exceed
//!   regions, assignment **wraps**, so two threads share a shard. That is fine
//!   here — the chunk table is indexed by shard, not by thread, and two threads
//!   in one shard simply share its `Mutex` as they do today.
//! * **[`crate::heap::bump`] (`RTS_BUMP=1`) — orthogonal, different object.**
//!   The recycler pools the *payload buffers* (`Entry::Vec`'s `Box<Vec<i64>>`);
//!   this stabilizes the *slots* that point at them. A recycled buffer changes
//!   which heap block an `Entry` owns; it never changes which slot the `Entry`
//!   lives in, and no address this module produces reaches the pool. They meet at
//!   exactly one line — `sweep_unmarked` calls `bump::recycle` while holding the
//!   shard lock — and that line's behaviour is unchanged because chunking only
//!   alters how the sweep *reaches* the slot, not what it does with it. Note the
//!   long-run direction makes them converge rather than conflict: `bump.rs` says
//!   a genuine bump pointer "arrives with the inline-slot object layout", and
//!   that layout is what puts field words *inside* the slot this module makes
//!   stable — at which point the payload buffer, and therefore the recycler, has
//!   nothing left to pool for plain objects.
//!
//! ## Where the old and new paths must agree
//!
//! Every one of these is a place a divergence would be a silent correctness bug,
//! so each is either shared code or covered by a test in [`store`]:
//!
//! | # | Invariant | How it is kept |
//! |---|---|---|
//! | 1 | Slot indices are dense and assigned in the same order | both paths append at `len`, both pop the same `free_list` |
//! | 2 | `len()` counts every slot ever appended, including freed ones | shared meaning; `free_list` holds indices, never shrinks `len` |
//! | 3 | Iteration order for sweep and for `live_handles_snapshot` | chunked iteration is index-ascending, same as `Vec` |
//! | 4 | Out-of-range index yields `None`, never a panic | both `get`/`get_mut` bounds-check against `len` |
//! | 5 | Generation transitions (`1` on first use, `next_generation` on reuse) | untouched in `handles.rs`, shared by both paths |
//! | 6 | A dropped table runs `cleanup_entry` on every live slot | `Drop` iterates via the same `iter_mut` |
//! | 7 | Handle encode/decode | untouched; this module never encodes a handle |
//!
//! TODO(measure): **no number is claimed for this module.** The ~2.2 ns/touch of
//! §5 3.1 and kernel H's H7/H10 rows describe the *probe's* model of this
//! storage, not this implementation, and 3.1's win is not collectable until 3.2
//! emits the load. Measure the knob's cost on the existing path (it should be
//! zero to within noise: one extra branch per slot access) before measuring
//! anything else.

pub mod chunks;
pub mod store;

use std::sync::OnceLock;

pub use store::{Slot, SlotStore};

/// `RTS_SLAB=1` — back each shard's slots with chunked, stable-address storage
/// instead of a `Vec` that reallocates.
///
/// OFF by default. This is a storage-representation change under the hot
/// allocation path; the old path must stay live and be the default until the
/// A/B is measured on one binary. Read once and cached: the choice must be
/// STABLE for the life of the process, because a shard that started chunked
/// cannot be re-read as a `Vec`.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_SLAB")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}

/// Base address of the flat chunk table, as an integer.
///
/// This is the one number Tier 3.2's emitted code needs in order to turn a
/// handle into an address without a call: it bakes this as an `iconst` (JIT) or
/// resolves this symbol (AOT) and then emits the five-instruction sequence in
/// this module's header. It is a `static` array, so the address is fixed for the
/// life of the process and safe to bake.
///
/// Returns `0` when the knob is OFF, which is a deliberate, checkable signal
/// rather than a silently-wrong pointer: with the `Vec` path live there is no
/// stable address to hand out, and codegen that receives `0` must emit the call
/// path. **A caller that skips that check gets a null-deref, not a wrong
/// answer** — the failure is loud by construction.
///
/// Deliberately a plain `pub fn` and **not** an `#[rtse::abi]` symbol yet. 3.1
/// has no codegen side, so a baked symbol here would be an unused row in the
/// symbol table that the gate would then have to carry. Tier 3.2 is the item
/// that needs it reachable from AOT and is the item that should declare it
/// (`#[rtse::abi]`, then `cargo run -p rts-symbol-baker`, then commit the
/// regenerated artefact). Never hand-write the `#[no_mangle]` name and never
/// hand-edit the baked table.
pub fn slab_chunk_table_base() -> u64 {
    if !enabled() {
        return 0;
    }
    chunks::table_base() as u64
}

/// Byte stride between two consecutive slots inside a chunk.
///
/// Exposed rather than hardcoded anywhere else: it is `size_of::<Slot>()`, a
/// number only the Rust compiler knows, and `RTS_CLASS_IMPLEMENTATION.md` §6.3
/// is explicit that layout constants get **one** definition in `rts-natives`
/// which the codegen imports — a second copy is the mirror-table failure the
/// single-source-of-truth rule exists to prevent.
///
/// Note for 3.2: this is not necessarily a power of two, so the address
/// computation is a multiply rather than a shift. Padding `Slot` up to 64 bytes
/// would buy the shift and one-slot-per-cache-line, at a memory cost nobody has
/// measured. TODO(measure) — do not pad on a hunch.
///
/// Plain `pub fn` for the same reason as [`slab_chunk_table_base`]: the symbol
/// is 3.2's to declare.
pub fn slab_slot_stride() -> u64 {
    core::mem::size_of::<Slot>() as u64
}
