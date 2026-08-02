//! Driver for kernel M — method call + `this` field access.
//!
//! The question: a `class P { x; y; sum() { return this.x + this.y } }` called
//! 3M times measures 26.3 ns/iter in the real engine. WHERE does that go — the
//! dispatch, the `this` tagging, or the field reads?

use crate::emit;
use crate::harness::{Check, Row, report};
use crate::poly;
use crate::slab;

use super::{ITERS_A, MASK, N_OBJS, SHAPE_ID};

pub fn kernel_m() {
    slab::sharded::reset();
    slab::unlocked::reset();
    slab::arena::reset();

    let mut payloads = Vec::with_capacity(N_OBJS);
    let mut payloads_arena = Vec::with_capacity(N_OBJS);
    for k in 0..N_OBJS as i64 {
        let x = poly::from_f64(k as f64) as i64;
        let y = poly::from_f64((k + 1) as f64) as i64;
        let p = slab::sharded::alloc_object(&[x, y], SHAPE_ID) as i64;
        let q = slab::unlocked::alloc_object(&[x, y], SHAPE_ID) as i64;
        assert_eq!(p, q, "the two slabs must hand out the same payloads");
        payloads.push(p);
        payloads_arena.push(slab::arena::alloc_object(&[x, y], SHAPE_ID) as i64);
    }

    let hdr_slab = [payloads.as_ptr() as i64, 0];
    let hdr_arena = [payloads_arena.as_ptr() as i64, slab::arena::base_addr()];
    let p_slab = hdr_slab.as_ptr() as i64;
    let p_arena = hdr_arena.as_ptr() as i64;

    // s += p.sum(), with p.x = k, p.y = k + 1.
    let expect: f64 = (0..ITERS_A).fold(0.0, |s, i| {
        let k = (i & MASK) as f64;
        s + k + (k + 1.0)
    });

    let specs: Vec<(&'static str, &'static str, emit::Compiled, i64)> = vec![
        (
            "M0 today",
            "real call, tagged this, 2 locked field calls, generic +",
            emit::kernel_m::m0_today(),
            p_slab,
        ),
        (
            "M1 +proven Repr",
            "real call, tagged this, 2 locked field calls, + INLINE",
            emit::kernel_m::m1_inline_arith(),
            p_slab,
        ),
        (
            "M2 +untagged this",
            "real call, this = raw payload (no band)",
            emit::kernel_m::m2_untagged_this(),
            p_slab,
        ),
        (
            "M3 +no lock",
            "real call, unlocked field calls",
            emit::kernel_m::m3_no_lock(),
            p_slab,
        ),
        (
            "M4 +this in register",
            "real call, this = ADDRESS, fields are loads",
            emit::kernel_m::m4_this_in_register(),
            p_arena,
        ),
        (
            "M5 +method inlined",
            "no call at all, IC shape guard + 2 loads",
            emit::kernel_m::m5_inlined_guarded(SHAPE_ID),
            p_arena,
        ),
        (
            "M6 +escape analysis",
            "object gone entirely",
            emit::kernel_m::m6_scalarized(),
            p_arena,
        ),
    ];

    report(
        "KERNEL M — method call + `this`: s = s + p.sum(), 1024 objects, 3M iterations",
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
    drop(payloads);
    drop(payloads_arena);
}
