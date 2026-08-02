//! Inline-slot block slabs — handle identity KEPT, `Box<Vec<i64>>` REMOVED.
//!
//! The shape the constraint census forces: the conservative GC scanner
//! (`collector/scan.rs`) and the threading model (`rts-threading-model.md`,
//! "payload = slot index, never a pointer") both require a class instance to
//! stay reachable through a HANDLE — shard + slot + generation — never through a
//! raw pointer baked into another structure. So RTS cannot copy Hermes's or
//! Dart's `movq [obj + off]` directly: it has no raw `obj`.
//!
//! What it CAN do is make handle→address computable in PURE IR:
//!
//! ```text
//! shard = payload & 31
//! idx   = payload >> 5
//! base  = load [SHARD_BASES + shard*8]     ; loop-invariant, hoistable
//! addr  = base + idx * STRIDE_BYTES
//! x     = load [addr + 8*(1 + slot)]
//! ```
//!
//! Object layout, `STRIDE_WORDS` words at a fixed stride so `idx * STRIDE` is a
//! shift: word 0 = shape id, words `1..=DIRECT_SLOTS` = inline fields (Hermes's
//! `DIRECT_PROPERTY_SLOTS`), last word = overflow pointer for the property bag
//! (`0` = none) — the fallback the census's HARD #4 needs, since
//! `(p as any).z = 1` is legal today and a fixed-offset struct has nowhere to
//! put `z`.
//!
//! Two strides are instantiated so the CACHE cost of a fixed stride is visible:
//! [`inline_slab`] (8 words = 64 B, one whole cache line per object) and
//! [`packed_slab`] (4 words = 32 B). A 2-field class in an 8-word stride wastes
//! 5 words, which halves how many objects fit in a cache line — a fixed stride
//! is a memory/indirection trade, not a free win.
//!
//! Each also publishes a **chunk table**: a flat `[shard][chunk] -> base` array
//! modelling the stable storage a real implementation needs (a `Vec` that
//! reallocs would invalidate every live object address mid-run). The point of
//! measuring it is that the two-level index collapses into ONE table load —
//! `table[shard * CHUNKS + chunk]` — so chunked storage costs ALU, not a second
//! dependent load.

use std::cell::UnsafeCell;
use std::sync::OnceLock;

use super::{N_SHARDS, SHARD_BITS, SHARD_MASK};

/// Objects per chunk. A real implementation would size this to a page-multiple;
/// what matters here is that it is a power of two so the split is shift+mask.
pub const CHUNK_BITS: u32 = 10;
pub const CHUNK_OBJS: usize = 1 << CHUNK_BITS;
pub const CHUNK_MASK: i64 = (CHUNK_OBJS as i64) - 1;
/// Chunks reserved per shard.
pub const CHUNKS: usize = 64;

macro_rules! block_slab {
    ($name:ident, $stride:expr) => {
        pub mod $name {
            use super::*;

            pub const STRIDE_WORDS: usize = $stride;
            pub const STRIDE_BYTES: i64 = (STRIDE_WORDS * 8) as i64;
            /// Words `1..=DIRECT_SLOTS` hold fields; the last word is the
            /// overflow pointer.
            pub const DIRECT_SLOTS: usize = STRIDE_WORDS - 2;
            pub const OVERFLOW_WORD: usize = STRIDE_WORDS - 1;

            const CAP_PER_SHARD: usize = CHUNKS * CHUNK_OBJS;

            struct Shards {
                words: UnsafeCell<Vec<Vec<i64>>>,
                /// `[shard] -> base`, what the flat-addressing IR loads.
                bases: UnsafeCell<[i64; N_SHARDS]>,
                /// `[shard * CHUNKS + chunk] -> base`, the chunk-list model.
                chunks: UnsafeCell<Vec<i64>>,
                next: UnsafeCell<[usize; N_SHARDS]>,
                rr: UnsafeCell<usize>,
            }
            // SAFETY: single-threaded probe (see the crate README caveats).
            unsafe impl Sync for Shards {}

            fn shards() -> &'static Shards {
                static S: OnceLock<Shards> = OnceLock::new();
                S.get_or_init(|| {
                    let mut words = Vec::with_capacity(N_SHARDS);
                    for _ in 0..N_SHARDS {
                        let mut v: Vec<i64> = Vec::new();
                        // Pre-reserved so the base never moves. A real
                        // implementation needs genuinely stable storage; see
                        // the README caveat — this precondition is NOT free.
                        v.resize(CAP_PER_SHARD * STRIDE_WORDS, 0);
                        words.push(v);
                    }
                    let mut bases = [0i64; N_SHARDS];
                    let mut chunks = vec![0i64; N_SHARDS * CHUNKS];
                    for (s, v) in words.iter().enumerate() {
                        let base = v.as_ptr() as i64;
                        bases[s] = base;
                        for c in 0..CHUNKS {
                            chunks[s * CHUNKS + c] =
                                base + (c * CHUNK_OBJS) as i64 * STRIDE_BYTES;
                        }
                    }
                    Shards {
                        words: UnsafeCell::new(words),
                        bases: UnsafeCell::new(bases),
                        chunks: UnsafeCell::new(chunks),
                        next: UnsafeCell::new([0; N_SHARDS]),
                        rr: UnsafeCell::new(0),
                    }
                })
            }

            /// Address of `[shard] -> base`.
            pub fn base_table_addr() -> i64 {
                // SAFETY: single-threaded probe; fixed after init.
                unsafe { (*shards().bases.get()).as_ptr() as i64 }
            }

            /// Address of `[shard * CHUNKS + chunk] -> base`.
            pub fn chunk_table_addr() -> i64 {
                // SAFETY: single-threaded probe; fixed after init.
                unsafe { (*shards().chunks.get()).as_ptr() as i64 }
            }

            /// Allocate `[shape_id, fields..., 0]`. Returns a handle payload
            /// encoded exactly like the real one: `(slot << 5) | shard`.
            pub fn alloc_object(fields: &[i64], shape_id: i64) -> u64 {
                assert!(fields.len() <= DIRECT_SLOTS, "more fields than direct slots");
                let s = shards();
                // SAFETY: single-threaded probe.
                let (words, next, rr) =
                    unsafe { (&mut *s.words.get(), &mut *s.next.get(), &mut *s.rr.get()) };
                let shard = *rr & (N_SHARDS - 1);
                *rr = rr.wrapping_add(1);
                let slot = next[shard];
                assert!(slot < CAP_PER_SHARD, "probe block slab exhausted");
                next[shard] = slot + 1;
                let off = slot * STRIDE_WORDS;
                let w = &mut words[shard];
                w[off] = shape_id;
                for (i, f) in fields.iter().enumerate() {
                    w[off + 1 + i] = *f;
                }
                w[off + OVERFLOW_WORD] = 0;
                ((slot as u64) << SHARD_BITS) | (shard as u64)
            }

            /// Point the object's overflow word at `bag`. Prices Hermes's
            /// `_sh_prload_indirect` / the property-bag arm.
            pub fn attach_overflow(payload: u64, bag: &mut [i64]) {
                let s = shards();
                // SAFETY: single-threaded probe.
                let w = unsafe { &mut *s.words.get() };
                let shard = (payload & SHARD_MASK) as usize;
                let slot = (payload >> SHARD_BITS) as usize;
                w[shard][slot * STRIDE_WORDS + OVERFLOW_WORD] = bag.as_ptr() as i64;
            }

            /// The call-shaped accessor: same body as the raw load the IR does,
            /// but behind an opaque `extern "C"` boundary. Isolates "the call"
            /// from "the indirection" the way `unlocked` isolates "the lock".
            pub fn get(payload: u64, index: i64) -> i64 {
                let s = shards();
                // SAFETY: single-threaded probe.
                let w = unsafe { &*s.words.get() };
                let shard = (payload & SHARD_MASK) as usize;
                let slot = (payload >> SHARD_BITS) as usize;
                w[shard][slot * STRIDE_WORDS + index as usize]
            }

            pub fn reset() {
                let s = shards();
                // SAFETY: single-threaded probe.
                let next = unsafe { &mut *s.next.get() };
                *next = [0; N_SHARDS];
            }
        }
    };
}

block_slab!(inline_slab, 8);
block_slab!(packed_slab, 4);

/// MOVABLE blocks — the regional-GC-compatible shape.
///
/// The flat slabs above derive an object's address from the HANDLE BITS
/// (`base[shard] + idx*STRIDE`). That is incompatible with the two things the
/// GC design actually plans:
///
/// - `rts-threading-model.md` — "**Promotion = moving slots**: re-home the
///   entries into shared shards and update the slots; live words keep their
///   meaning". Re-homing into a *shared shard* changes the shard bits, so a
///   handle-derived address goes stale in every live copy of the word.
/// - `gc-generational-design.md` — "moving = update slot→address in the
///   HandleTable (**the indirection makes this ≈ free**, no pointer-patching)".
///   That indirection is precisely what the flat slabs delete.
///
/// So the movable form keeps ONE indirection, but as a plain word load rather
/// than a `Box<Vec<i64>>` deref behind an opaque call holding a shard `Mutex`:
///
/// ```text
/// stab  = load [SLOT_TABLES + shard*8]   ; loop-invariant
/// block = load [stab + idx*8]            ; THE indirection — one word
/// x     = load [block + 8*(1 + slot)]
/// ```
///
/// Moving an object is then: copy the block, store its new address into that
/// one word. The handle never changes, no live word is rewritten — the property
/// the whole value model was built for, kept, at the price of one dependent
/// load instead of an address computation.
pub mod moving_slab {
    use super::*;

    pub const STRIDE_WORDS: usize = 8;
    pub const STRIDE_BYTES: i64 = (STRIDE_WORDS * 8) as i64;
    pub const DIRECT_SLOTS: usize = STRIDE_WORDS - 2;
    pub const OVERFLOW_WORD: usize = STRIDE_WORDS - 1;

    const CAP_PER_SHARD: usize = 1 << 16;

    struct Shards {
        /// Object storage, per shard. Blocks live here; nothing addresses them
        /// by index — only the slot table points at them.
        blocks: UnsafeCell<Vec<Vec<i64>>>,
        /// `[shard] -> &slots[shard][0]`, one indirection level for the IR.
        slot_tables: UnsafeCell<[i64; N_SHARDS]>,
        /// `slots[shard][idx]` = address of the object's block. THE indirection.
        slots: UnsafeCell<Vec<Vec<i64>>>,
        next: UnsafeCell<[usize; N_SHARDS]>,
        rr: UnsafeCell<usize>,
    }
    // SAFETY: single-threaded probe.
    unsafe impl Sync for Shards {}

    fn shards() -> &'static Shards {
        static S: OnceLock<Shards> = OnceLock::new();
        S.get_or_init(|| {
            let mut blocks = Vec::with_capacity(N_SHARDS);
            let mut slots = Vec::with_capacity(N_SHARDS);
            for _ in 0..N_SHARDS {
                let mut b: Vec<i64> = Vec::new();
                b.resize(CAP_PER_SHARD * STRIDE_WORDS, 0);
                blocks.push(b);
                let mut s: Vec<i64> = Vec::new();
                s.resize(CAP_PER_SHARD, 0);
                slots.push(s);
            }
            let mut slot_tables = [0i64; N_SHARDS];
            for (i, s) in slots.iter().enumerate() {
                slot_tables[i] = s.as_ptr() as i64;
            }
            Shards {
                blocks: UnsafeCell::new(blocks),
                slot_tables: UnsafeCell::new(slot_tables),
                slots: UnsafeCell::new(slots),
                next: UnsafeCell::new([0; N_SHARDS]),
                rr: UnsafeCell::new(0),
            }
        })
    }

    pub fn slot_table_addr() -> i64 {
        // SAFETY: single-threaded probe; fixed after init.
        unsafe { (*shards().slot_tables.get()).as_ptr() as i64 }
    }

    /// Base of ONE region's slot table — the address a single-region build
    /// bakes as an immediate, so the per-shard table load disappears and only
    /// THE indirection is left. Pairs with [`alloc_in_region`].
    pub fn region_slot_table_addr() -> i64 {
        let s = shards();
        // SAFETY: single-threaded probe; capacity reserved so this is stable.
        unsafe { (&*s.slots.get())[0].as_ptr() as i64 }
    }

    /// Allocate into region 0 and return the slot index UNSHIFTED — a
    /// single-region encoding spends no bits on shard routing.
    pub fn alloc_in_region(fields: &[i64], shape_id: i64) -> u64 {
        let p = alloc_into(0, fields, shape_id);
        p >> SHARD_BITS
    }

    pub fn alloc_object(fields: &[i64], shape_id: i64) -> u64 {
        let s = shards();
        // SAFETY: single-threaded probe.
        let rr = unsafe { &mut *s.rr.get() };
        let shard = *rr & (N_SHARDS - 1);
        *rr = rr.wrapping_add(1);
        alloc_into(shard, fields, shape_id)
    }

    fn alloc_into(shard: usize, fields: &[i64], shape_id: i64) -> u64 {
        assert!(fields.len() <= DIRECT_SLOTS, "more fields than direct slots");
        let s = shards();
        // SAFETY: single-threaded probe.
        let (blocks, slots, next) = unsafe {
            (&mut *s.blocks.get(), &mut *s.slots.get(), &mut *s.next.get())
        };
        let idx = next[shard];
        assert!(idx < CAP_PER_SHARD, "probe moving slab exhausted");
        next[shard] = idx + 1;
        let off = idx * STRIDE_WORDS;
        let b = &mut blocks[shard];
        b[off] = shape_id;
        for (i, f) in fields.iter().enumerate() {
            b[off + 1 + i] = *f;
        }
        b[off + OVERFLOW_WORD] = 0;
        let addr = unsafe { b.as_ptr().add(off) } as i64;
        slots[shard][idx] = addr;
        ((idx as u64) << SHARD_BITS) | (shard as u64)
    }

    /// Move an object's block and repoint its slot — the whole reason the
    /// indirection exists. One word written; the handle is untouched and no
    /// live PolyValue word is rewritten.
    pub fn relocate(payload: u64, to: &mut [i64]) {
        let s = shards();
        // SAFETY: single-threaded probe.
        let (blocks, slots) = unsafe { (&*s.blocks.get(), &mut *s.slots.get()) };
        let shard = (payload & SHARD_MASK) as usize;
        let idx = (payload >> SHARD_BITS) as usize;
        let off = idx * STRIDE_WORDS;
        to[..STRIDE_WORDS].copy_from_slice(&blocks[shard][off..off + STRIDE_WORDS]);
        slots[shard][idx] = to.as_ptr() as i64;
    }

    /// CONSTRUCTION-side allocation for kernel W: bump a block, install the
    /// slot-table word, and hand back BOTH the handle payload and the block's
    /// address — **without touching the field words**.
    ///
    /// [`alloc_object`] fills the fields itself, which is the wrong shape for a
    /// write ladder: the whole question C0 asks is what the FIELD STORES cost
    /// when the emitted code does them. So this stops after the shape word and
    /// the overflow word (both of which a real allocator writes regardless), and
    /// the JITted kernel stores the fields.
    ///
    /// Returning the address as well as the payload is what lets kernel W
    /// separate "the allocator handed the constructor an address" (W1) from
    /// "the store site recomputed it from the handle" (W2) — one variable.
    ///
    /// Deliberately duplicates ~8 lines of `alloc_into` rather than refactoring
    /// it: `alloc_into` backs the already-published kernel H numbers and must not
    /// change behaviour.
    fn alloc_block_into(shard: usize, shape_id: i64) -> (u64, i64) {
        let s = shards();
        // SAFETY: single-threaded probe.
        let (blocks, slots, next) = unsafe {
            (&mut *s.blocks.get(), &mut *s.slots.get(), &mut *s.next.get())
        };
        let idx = next[shard];
        assert!(idx < CAP_PER_SHARD, "probe moving slab exhausted");
        next[shard] = idx + 1;
        let off = idx * STRIDE_WORDS;
        let b = &mut blocks[shard];
        b[off] = shape_id;
        b[off + OVERFLOW_WORD] = 0;
        let addr = unsafe { b.as_ptr().add(off) } as i64;
        slots[shard][idx] = addr;
        (((idx as u64) << SHARD_BITS) | (shard as u64), addr)
    }

    /// Sharded round-robin form — the same routing [`alloc_object`] uses.
    pub fn alloc_block(shape_id: i64) -> (u64, i64) {
        let s = shards();
        // SAFETY: single-threaded probe.
        let rr = unsafe { &mut *s.rr.get() };
        let shard = *rr & (N_SHARDS - 1);
        *rr = rr.wrapping_add(1);
        alloc_block_into(shard, shape_id)
    }

    /// Single-region form: region 0, payload UNSHIFTED (no bits spent on shard
    /// routing), pairing with [`region_slot_table_addr`]. Note the region arm is
    /// capped at `CAP_PER_SHARD` blocks, which is why kernel W's iteration count
    /// is bounded — a probe capacity limit, not a statement about the design.
    pub fn alloc_block_in_region(shape_id: i64) -> (u64, i64) {
        let (p, addr) = alloc_block_into(0, shape_id);
        (p >> SHARD_BITS, addr)
    }

    pub fn get(payload: u64, index: i64) -> i64 {
        let s = shards();
        // SAFETY: single-threaded probe.
        let slots = unsafe { &*s.slots.get() };
        let shard = (payload & SHARD_MASK) as usize;
        let idx = (payload >> SHARD_BITS) as usize;
        let block = slots[shard][idx] as *const i64;
        // SAFETY: the slot points at a live block for the probe's lifetime.
        unsafe { *block.add(index as usize) }
    }

    pub fn reset() {
        let s = shards();
        // SAFETY: single-threaded probe.
        let next = unsafe { &mut *s.next.get() };
        *next = [0; N_SHARDS];
    }
}

/// A single thread-local REGION: no shard routing, one base the JIT can bake as
/// an `iconst` (or keep in a reserved register, which is what Dart does with
/// `GDT` and V8 does with pointer compression). The threading model already
/// calls the shards "proto-regions", so this row prices the upside of finishing
/// that: the shard-base table load disappears entirely.
///
/// The payload is the slot index directly — still not a pointer, so the GC and
/// the region-migration story are unchanged.
pub mod region_slab {
    use super::*;

    pub const STRIDE_WORDS: usize = 8;
    pub const STRIDE_BYTES: i64 = (STRIDE_WORDS * 8) as i64;
    pub const DIRECT_SLOTS: usize = STRIDE_WORDS - 2;
    pub const OVERFLOW_WORD: usize = STRIDE_WORDS - 1;
    const CAP: usize = 1 << 20;

    struct Region {
        words: UnsafeCell<Vec<i64>>,
        next: UnsafeCell<usize>,
    }
    // SAFETY: single-threaded probe.
    unsafe impl Sync for Region {}

    fn region() -> &'static Region {
        static R: OnceLock<Region> = OnceLock::new();
        R.get_or_init(|| {
            let mut v: Vec<i64> = Vec::new();
            v.resize(CAP * STRIDE_WORDS, 0);
            Region {
                words: UnsafeCell::new(v),
                next: UnsafeCell::new(0),
            }
        })
    }

    /// The base the kernel bakes as an immediate.
    pub fn base_addr() -> i64 {
        // SAFETY: single-threaded probe; capacity reserved so this is stable.
        unsafe { (*region().words.get()).as_ptr() as i64 }
    }

    pub fn alloc_object(fields: &[i64], shape_id: i64) -> u64 {
        assert!(fields.len() <= DIRECT_SLOTS, "more fields than direct slots");
        let r = region();
        // SAFETY: single-threaded probe.
        let (w, next) = unsafe { (&mut *r.words.get(), &mut *r.next.get()) };
        let slot = *next;
        assert!(slot < CAP, "probe region exhausted");
        *next += 1;
        let off = slot * STRIDE_WORDS;
        w[off] = shape_id;
        for (i, f) in fields.iter().enumerate() {
            w[off + 1 + i] = *f;
        }
        w[off + OVERFLOW_WORD] = 0;
        slot as u64
    }

    pub fn reset() {
        let r = region();
        // SAFETY: single-threaded probe.
        unsafe { *r.next.get() = 0 };
    }
}
