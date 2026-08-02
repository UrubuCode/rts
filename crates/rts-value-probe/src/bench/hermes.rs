//! Driver for kernel H — today's object management vs the Static-Hermes-shaped
//! one, under RTS's handle-identity constraint. See `emit/kernel_h.rs`.

use crate::emit;
use crate::harness::{Check, Row, report};
use crate::poly;
use crate::slab;
use crate::slab::blocks::{inline_slab, moving_slab, packed_slab, region_slab};

use super::{ITERS_A, MASK, N_OBJS, SHAPE_ID};

pub fn kernel_h() {
    slab::sharded::reset();
    slab::unlocked::reset();
    inline_slab::reset();
    packed_slab::reset();
    region_slab::reset();
    moving_slab::reset();

    let mut payloads_slab = Vec::with_capacity(N_OBJS);
    let mut payloads_inline = Vec::with_capacity(N_OBJS);
    let mut payloads_packed = Vec::with_capacity(N_OBJS);
    let mut payloads_region = Vec::with_capacity(N_OBJS);
    let mut payloads_moving = Vec::with_capacity(N_OBJS);
    let mut payloads_movreg = Vec::with_capacity(N_OBJS);
    // One overflow bag per object for the H5 row. Kept alive for the whole run.
    let mut bags: Vec<Vec<i64>> = Vec::with_capacity(N_OBJS);

    for k in 0..N_OBJS as i64 {
        let x = poly::from_f64(k as f64) as i64;
        let y = poly::from_f64((k + 1) as f64) as i64;
        let p = slab::sharded::alloc_object(&[x, y], SHAPE_ID) as i64;
        let q = slab::unlocked::alloc_object(&[x, y], SHAPE_ID) as i64;
        assert_eq!(p, q, "the two slabs must hand out the same payloads");
        payloads_slab.push(p);
        let ip = inline_slab::alloc_object(&[x, y], SHAPE_ID) as i64;
        let pp = packed_slab::alloc_object(&[x, y], SHAPE_ID) as i64;
        assert_eq!(ip, pp, "both strides must hand out the same payloads");
        payloads_inline.push(ip);
        payloads_packed.push(pp);
        payloads_region.push(region_slab::alloc_object(&[x, y], SHAPE_ID) as i64);
        payloads_moving.push(moving_slab::alloc_object(&[x, y], SHAPE_ID) as i64);
        payloads_movreg.push(moving_slab::alloc_in_region(&[x, y], SHAPE_ID) as i64);
        bags.push(vec![x, y]);
    }

    // Prove the movable form actually delivers what regional promotion and a
    // copying nursery need: RELOCATE every object's block into a to-space, then
    // read it back through the SAME handle. If the handle survived a move,
    // H10's checksum is evidence for the whole design, not just a load count.
    //
    // The to-space is ONE contiguous allocation, evacuated in handle order —
    // what a copying collector actually produces. An earlier version gave each
    // block its own `vec![0; 8]`; 1024 scattered mallocs made H10 swing 4.4↔8.2
    // ns between runs, which measured the allocator, not the design.
    let mut tospace: Vec<i64> = vec![0; N_OBJS * moving_slab::STRIDE_WORDS];
    for i in 0..N_OBJS {
        let lo = i * moving_slab::STRIDE_WORDS;
        let hi = lo + moving_slab::STRIDE_WORDS;
        moving_slab::relocate(payloads_moving[i] as u64, &mut tospace[lo..hi]);
        assert_eq!(
            moving_slab::get(payloads_moving[i] as u64, 0),
            SHAPE_ID,
            "handle must still resolve after the block moved"
        );
    }
    // Attach the bags only after `bags` has stopped growing — a `Vec` realloc
    // would move every element and invalidate the pointers stored in the slab.
    for (i, bag) in bags.iter_mut().enumerate() {
        inline_slab::attach_overflow(payloads_inline[i] as u64, bag);
    }

    let base_table = inline_slab::base_table_addr();
    let hdr_slab = [payloads_slab.as_ptr() as i64, base_table];
    let hdr_inline = [payloads_inline.as_ptr() as i64, base_table];
    let hdr_chunked = [payloads_inline.as_ptr() as i64, inline_slab::chunk_table_addr()];
    let hdr_packed = [payloads_packed.as_ptr() as i64, packed_slab::base_table_addr()];
    let hdr_region = [payloads_region.as_ptr() as i64, 0];
    let hdr_moving = [payloads_moving.as_ptr() as i64, moving_slab::slot_table_addr()];
    let hdr_movreg = [payloads_movreg.as_ptr() as i64, 0];
    let p_slab = hdr_slab.as_ptr() as i64;
    let p_inline = hdr_inline.as_ptr() as i64;
    let p_chunked = hdr_chunked.as_ptr() as i64;
    let p_packed = hdr_packed.as_ptr() as i64;
    let p_region = hdr_region.as_ptr() as i64;
    let p_moving = hdr_moving.as_ptr() as i64;
    let p_movreg = hdr_movreg.as_ptr() as i64;

    // s += p.x + p.y, with p.x = k, p.y = k + 1 — same workload as kernel M.
    let expect: f64 = (0..ITERS_A).fold(0.0, |s, i| {
        let k = (i & MASK) as f64;
        s + k + (k + 1.0)
    });

    let specs: Vec<(&'static str, &'static str, emit::Compiled, i64)> = vec![
        (
            "H0 today",
            "call __rtsn_vec_get_by_payload: shard Mutex + Box<Vec<i64>>",
            emit::kernel_h::h0_today(),
            p_slab,
        ),
        (
            "H1 -shard Mutex",
            "same call, lock removed",
            emit::kernel_h::h1_no_lock(),
            p_slab,
        ),
        (
            "H2 -Box indirection",
            "inline-slot slab, STILL an opaque call",
            emit::kernel_h::h2_inline_slots_call(),
            p_inline,
        ),
        (
            "H3 -the call (HERMES)",
            "handle->addr in pure IR + 2 loads, guarded unbox",
            emit::kernel_h::h3_pure_ir_load(),
            p_inline,
        ),
        (
            "H4 +Repr on the field",
            "H3 with the tag guard gone (field proven Float64)",
            emit::kernel_h::h4_repr_proven(),
            p_inline,
        ),
        (
            "H5 overflow bag",
            "the (p as any).z fallback: _sh_prload_indirect shape",
            emit::kernel_h::h5_overflow_bag(),
            p_inline,
        ),
        (
            "H6 escape analysis",
            "object gone entirely — the floor",
            emit::kernel_h::h6_scalarized(),
            p_inline,
        ),
        (
            "H7 chunked storage",
            "H4 through a chunk table — the stable-storage precondition",
            emit::kernel_h::h7_chunked(),
            p_chunked,
        ),
        (
            "H8 packed stride 32B",
            "H4 with a 4-word stride instead of 8 — the cache cost",
            emit::kernel_h::h8_packed_stride(),
            p_packed,
        ),
        (
            "H9 region, const base",
            "no shard routing, base is an iconst — finished threading model",
            emit::kernel_h::h9_region_const_base(),
            p_region,
        ),
        (
            "H10 MOVABLE (regional)",
            "one slot indirection: blocks RELOCATED, handles unchanged",
            emit::kernel_h::h10_movable(),
            p_moving,
        ),
        (
            "H11 movable + region",
            "H10 with the slot table's base as an iconst",
            emit::kernel_h::h11_movable_region(),
            p_movreg,
        ),
    ];

    report(
        "KERNEL H — object management: today vs Static-Hermes-shaped, s = s + p.x + p.y, 3M iters",
        ITERS_A,
        expect,
        Check::Poly,
        specs
            .into_iter()
            .map(|(name, detail, c, hdr)| {
                Row::new(name, detail, move || (c.f)(ITERS_A, hdr, MASK))
            })
            .collect(),
    );
    drop(payloads_slab);
    drop(payloads_packed);
    drop(payloads_region);
    drop(payloads_moving);
    drop(payloads_movreg);
    drop(tospace);
    drop(payloads_inline);
    drop(bags);
}
