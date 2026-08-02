//! The allocation trampolines kernel W calls — the CONSTRUCTION half of the
//! probe. Reads already have theirs (`probe_vec_get_locked` / `probe_inline_get`
//! in `rt/mod.rs`); nothing here existed because nothing had priced a write.
//!
//! All are `#[inline(never)] extern "C"` for the same reason as the read ones:
//! the JIT resolves them through `JITBuilder::symbol`, so they are opaque calls
//! Cranelift cannot see into — which is what the real `__rtsn_*` trampolines are.

use crate::slab;
use crate::slab::blocks::moving_slab;

/// `__rtsn_vec_new_object` — allocate the object with NO fields, so the emitted
/// code fills them one `push` at a time. That is what `obj.rs:97-116` does today
/// (`vec_new_object` then one `__rtsn_vec_push_by_payload` per field), and it is
/// the baseline W0 has to reproduce.
///
/// Deliberately separate from [`crate::rt::probe_alloc_locked`], which takes the
/// two field values up front: kernel B needed that shape, kernel W needs the
/// empty one, and the existing symbol backs published numbers.
#[inline(never)]
pub extern "C" fn probe_alloc_locked_empty(shape: i64) -> i64 {
    slab::sharded::alloc_object(&[], shape) as i64
}

/// Movable-block allocation, returning the BLOCK ADDRESS: bump the block,
/// install the slot-table word, hand the constructor a pointer. No lock, no
/// `Box<Vec<i64>>`. Prices the case where allocation and initialization are
/// fused — the allocator's return value IS the store site's base.
#[inline(never)]
pub extern "C" fn probe_block_alloc_addr(shape: i64) -> i64 {
    moving_slab::alloc_block(shape).1
}

/// Same allocation, returning the HANDLE PAYLOAD instead. The store site then
/// has to recompute the address (slot table → THE indirection → block), which
/// is what a separately-lowered `this.x = v` must do. The delta against
/// [`probe_block_alloc_addr`] is that recomputation and nothing else.
#[inline(never)]
pub extern "C" fn probe_block_alloc(shape: i64) -> i64 {
    moving_slab::alloc_block(shape).0 as i64
}

/// The single-region form: no shard routing, payload unshifted, so the slot
/// table's base is an `iconst` in the emitted code (C5 / H11).
#[inline(never)]
pub extern "C" fn probe_region_alloc(shape: i64) -> i64 {
    moving_slab::alloc_block_in_region(shape).0 as i64
}

/// Allocation with every field supplied UP FRONT — one pass over the block
/// instead of allocate-then-store-each. Real `new` knows all its arguments at
/// the call site, so this is available to the engine whenever the constructor
/// body is inlined and does nothing but assign parameters to fields.
///
/// Arity is fixed at kernel W's field count (4). A variadic form would need a
/// pointer to a spilled argument array, which is a different (and slower)
/// design — pricing it is out of scope here.
#[inline(never)]
pub extern "C" fn probe_block_alloc_filled(
    shape: i64,
    f0: i64,
    f1: i64,
    f2: i64,
    f3: i64,
) -> i64 {
    moving_slab::alloc_object(&[f0, f1, f2, f3], shape) as i64
}

pub fn symbols() -> Vec<(&'static str, *const u8)> {
    vec![
        (
            "probe_alloc_locked_empty",
            probe_alloc_locked_empty as *const u8,
        ),
        ("probe_block_alloc_addr", probe_block_alloc_addr as *const u8),
        ("probe_block_alloc", probe_block_alloc as *const u8),
        ("probe_region_alloc", probe_region_alloc as *const u8),
        (
            "probe_block_alloc_filled",
            probe_block_alloc_filled as *const u8,
        ),
    ]
}
