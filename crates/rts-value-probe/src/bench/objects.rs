//! Drivers for kernels A (field read + arithmetic) and B (allocation).

use crate::emit;
use crate::harness::{Check, Row, report};
use crate::poly;
use crate::slab;

use super::{ITERS_A, ITERS_B, MASK, N_OBJS, SHAPE_ID};

pub fn kernel_a() {
    slab::sharded::reset();
    slab::unlocked::reset();
    slab::arena::reset();
    let mut payloads_locked = Vec::with_capacity(N_OBJS);
    let mut payloads_arena = Vec::with_capacity(N_OBJS);
    for k in 0..N_OBJS as i64 {
        let x = poly::from_f64(k as f64) as i64;
        let y = poly::from_f64((k + 1) as f64) as i64;
        let p = slab::sharded::alloc_object(&[x, y], SHAPE_ID) as i64;
        let q = slab::unlocked::alloc_object(&[x, y], SHAPE_ID) as i64;
        assert_eq!(p, q, "the two slabs must hand out the same payloads");
        payloads_locked.push(p);
        payloads_arena.push(slab::arena::alloc_object(&[x, y], SHAPE_ID) as i64);
    }

    let hdr_slab = [payloads_locked.as_ptr() as i64, 0];
    let hdr_arena = [payloads_arena.as_ptr() as i64, slab::arena::base_addr()];
    let p_slab = hdr_slab.as_ptr() as i64;
    let p_arena = hdr_arena.as_ptr() as i64;

    let expect: f64 = (0..ITERS_A).fold(0.0, |s, i| {
        let k = (i & MASK) as f64;
        s + k * (k + 1.0)
    });

    let specs: Vec<(&'static str, &'static str, emit::Compiled, i64)> = vec![
        (
            "A0 today",
            "locked call x2 + generic call x2",
            emit::kernel_a::a0_current(),
            p_slab,
        ),
        (
            "A1 +proven Repr",
            "locked call x2, arith INLINE",
            emit::kernel_a::a1_proven_repr(),
            p_slab,
        ),
        (
            "A2 +inline guard",
            "locked call x2, guarded generic",
            emit::kernel_a::a2_inline_guard(),
            p_slab,
        ),
        (
            "A3 +no lock",
            "unlocked call x2, arith inline",
            emit::kernel_a::a3_unlocked_call(),
            p_slab,
        ),
        (
            "A4 +direct load",
            "no call at all, arith inline",
            emit::kernel_a::a4_raw_load(),
            p_arena,
        ),
        (
            "A4g +shape guard",
            "A4 + IC shape compare (honest IC hit)",
            emit::kernel_a::a4g_raw_load_guarded(SHAPE_ID),
            p_arena,
        ),
        (
            "A5 +escape analysis",
            "object gone entirely",
            emit::kernel_a::a5_scalarized(),
            p_arena,
        ),
    ];

    report(
        "KERNEL A — object field read: s = s + p.x * p.y, 3M iterations, no allocation",
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
    drop(payloads_locked);
    drop(payloads_arena);
}

pub fn kernel_b() {
    let hdr = [0i64, slab::arena::base_addr()];
    let p = hdr.as_ptr() as i64;
    let expect: f64 = (0..ITERS_B).fold(0.0, |s, i| {
        let k = i as f64;
        s + k * (k + 1.0)
    });

    let b0 = emit::kernel_b::b0_current();
    let b1 = emit::kernel_b::b1_bump_direct();
    let b0f = emit::kernel_b::b0f_full_call_profile();
    let b1b = emit::kernel_b::b1b_bump_direct_barrier();
    let b2 = emit::kernel_b::b2_scalarized();

    report(
        "KERNEL B — allocation: const p = new P(i, i+1); s = s + p.x * p.y, 1M iterations",
        ITERS_B,
        expect,
        Check::Poly,
        vec![
            Row::new(
                "B0 today",
                "locked slab alloc + locked reads + generic ops",
                move || (b0.f)(ITERS_B, p, SHAPE_ID),
            )
            .with_setup(slab::sharded::reset),
            Row::new(
                "B0f FULL engine call profile (11 calls)",
                "the calibration row — does the probe reproduce 577 ns/iter?",
                move || (b0f.f)(ITERS_B, p, SHAPE_ID),
            )
            .with_setup(slab::sharded::reset),
            Row::new(
                "B1 bump+direct",
                "arena bump alloc + direct load + inline arith",
                move || (b1.f)(ITERS_B, p, SHAPE_ID),
            )
            .with_setup(slab::arena::reset),
            Row::new(

                "B1b bump+direct+card-mark barrier",

                "the honest end state: no lock, but a real write barrier",

                move || (b1b.f)(ITERS_B, p, SHAPE_ID),

            )

            .with_setup(slab::arena::reset),

            Row::new("B2 escape analysis", "no allocation at all", move || {
                (b2.f)(ITERS_B, p, SHAPE_ID)
            }),
        ],
    );
}
