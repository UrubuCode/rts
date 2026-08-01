//! Driver for kernel STRUCT — does a static struct layout beat the shaped object?

use crate::emit::kernel_struct::{t0_today, t1_shaped_direct, t2_static_struct, t2g_struct_guarded,
    t3_scalarized};
use crate::harness::{Check, Row, report};
use crate::poly;
use crate::slab;

use super::{ITERS_A, MASK, N_OBJS, SHAPE_ID};

pub fn kernel_struct() {
    slab::sharded::reset();
    slab::arena::reset();

    // Three layouts, all bump-allocated in the same arena so addressing is
    // identical and only the LAYOUT differs.
    let mut payloads_slab = Vec::with_capacity(N_OBJS);
    let mut off_shaped = Vec::with_capacity(N_OBJS);
    let mut off_struct = Vec::with_capacity(N_OBJS);
    let mut off_guarded = Vec::with_capacity(N_OBJS);

    for k in 0..N_OBJS as i64 {
        let xw = poly::from_f64(k as f64) as i64;
        let yw = poly::from_f64((k + 1) as f64) as i64;
        payloads_slab.push(slab::sharded::alloc_object(&[xw, yw], SHAPE_ID) as i64);
        // shaped: [shape_id][x][y] — a PolyValue double IS the f64 bits, so the
        // same words serve the struct layout; that is the point being tested.
        off_shaped.push(slab::arena::alloc_object(&[xw, yw], SHAPE_ID) as i64);
        // struct: [x][y], no header at all.
        off_struct.push(slab::arena::alloc_raw(&[xw, yw]) as i64);
        // guarded: [class_id][x][y].
        off_guarded.push(slab::arena::alloc_raw(&[SHAPE_ID, xw, yw]) as i64);
    }

    let base = slab::arena::base_addr();
    let h_slab = [payloads_slab.as_ptr() as i64, 0];
    let h_shaped = [off_shaped.as_ptr() as i64, base];
    let h_struct = [off_struct.as_ptr() as i64, base];
    let h_guarded = [off_guarded.as_ptr() as i64, base];
    let (p0, p1, p2, p3) = (
        h_slab.as_ptr() as i64,
        h_shaped.as_ptr() as i64,
        h_struct.as_ptr() as i64,
        h_guarded.as_ptr() as i64,
    );

    let expect: f64 = (0..ITERS_A).fold(0.0, |s, i| {
        let k = (i & MASK) as f64;
        s + k * (k + 1.0)
    });

    let t0 = t0_today();
    let t1 = t1_shaped_direct();
    let t2 = t2_static_struct();
    let t2g = t2g_struct_guarded(SHAPE_ID);
    let t3 = t3_scalarized();

    report(
        "KERNEL STRUCT — does a static struct layout beat the shaped object? 3M iterations",
        ITERS_A,
        expect,
        Check::Poly,
        vec![
            Row::new("T0 today", "slab + locked calls + generic arith", move || {
                (t0.f)(ITERS_A, p0, MASK)
            }),
            Row::new(
                "T1 shaped, direct load",
                "[shape][x][y], PolyValue words, bitcast + inline arith",
                move || (t1.f)(ITERS_A, p1, MASK),
            ),
            Row::new(
                "T2 static struct",
                "[x][y] raw f64, no header, no tag — the S1+S2 claim",
                move || (t2.f)(ITERS_A, p2, MASK),
            ),
            Row::new(
                "T2g struct + class guard",
                "[cid][x][y] — the honest cost of keeping JS semantics",
                move || (t2g.f)(ITERS_A, p3, MASK),
            ),
            Row::new("T3 scalarized", "object never existed — the ceiling", move || {
                (t3.f)(ITERS_A, p2, MASK)
            }),
        ],
    );
    drop((payloads_slab, off_shaped, off_struct, off_guarded));
}
