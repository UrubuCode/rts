//! Driver for kernel W — construction + field writes on the movable block
//! layout. `RTS_CLASS_IMPLEMENTATION.md` §7 C0. See `emit/kernel_w.rs`.
//!
//! Unlike kernel H, this kernel ALLOCATES on every iteration, so every row needs
//! its slabs rewound between timed runs. `Row::with_setup` exists for exactly
//! this: the rewind runs outside the timer, because the real engine does not
//! perform one per loop.

use crate::emit;
use crate::emit::kernel_w::FIELDS;
use crate::harness::{Check, Row, report};
use crate::slab;
use crate::slab::blocks::moving_slab;
use crate::slab::cards;

use super::{ITERS_W, SHAPE_ID};

/// Rewind everything a W row touches. Cheap and unconditional: doing it for
/// every row (rather than per-row) keeps the rows' setup cost identical, which
/// matters because the harness runs setup between every timed repetition.
fn rewind() {
    slab::sharded::reset();
    moving_slab::reset();
    cards::reset();
}

pub fn kernel_w() {
    rewind();

    let hdr = [moving_slab::slot_table_addr(), cards::table_addr()];
    let p = hdr.as_ptr() as i64;

    // Same accumulation order as the emitted loop: all FIELDS of one object, in
    // slot order, before moving to the next object. Every value is an integral
    // f64 well under 2^53, so this is exact and the harness's 1e-9 relative
    // tolerance is not doing any work.
    let expect: f64 = (0..ITERS_W).fold(0.0, |mut s, i| {
        for j in 0..FIELDS {
            s += (i + j) as f64;
        }
        s
    });

    let specs: Vec<(&'static str, &'static str, emit::Compiled)> = vec![
        (
            "W0 today",
            "vec_new_object + one LOCKED push per field, locked read-back",
            emit::kernel_w::w0_today(),
        ),
        (
            "W1 block alloc, direct store",
            "-lock, -Box: allocator returns the block address",
            emit::kernel_w::w1_block_direct(),
        ),
        (
            "W2 store via handle",
            "-the handed-back address: handle->block in pure IR (the C2 number)",
            emit::kernel_w::w2_store_via_handle(),
        ),
        (
            "W3 +card-mark barrier",
            "W2 + one unconditional card mark per field store",
            emit::kernel_w::w3_store_barrier(),
        ),
        (
            "W4 region, const base",
            "W2 with no shard routing, slot-table base as an iconst",
            emit::kernel_w::w4_region_const_base(),
        ),
        (
            "W5 filled at alloc",
            "-the separate store pass: fields supplied to the allocator",
            emit::kernel_w::w5_filled_at_alloc(),
        ),
    ];

    report(
        "KERNEL W — construction + field writes: new P(f0..f3) then read back, 50k objects",
        ITERS_W,
        expect,
        Check::Poly,
        specs
            .into_iter()
            .map(|(name, detail, c)| {
                Row::new(name, detail, move || (c.f)(ITERS_W, p, SHAPE_ID))
                    .with_setup(rewind)
            })
            .collect(),
    );
}
