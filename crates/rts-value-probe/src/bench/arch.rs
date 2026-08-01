//! Driver for kernel ARCH — alias regions (A3), the error poll (A5), the
//! calling convention (A6).

use crate::emit::kernel_arch as ka;
use crate::harness::{Check, Row, report};
use crate::poly;
use crate::slab;

use super::{ITERS_A, SHAPE_ID};

pub fn kernel_arch() {
    slab::arena::reset();
    // Slot 0 of the arena doubles as the "error flag" word for A5, so it must
    // be zero (no pending error) — `alloc_raw` puts it there deliberately.
    slab::arena::alloc_raw(&[0]);

    const X: f64 = 2.0;
    const Y: f64 = 3.0;
    let obj = slab::arena::alloc_object(
        &[poly::from_f64(X) as i64, poly::from_f64(Y) as i64],
        SHAPE_ID,
    ) as i64;

    let hdr = [0i64, slab::arena::base_addr()];
    let p = hdr.as_ptr() as i64;

    // --- A3: does an alias region let a store to B not clobber a read of A? ---
    let a30 = ka::a3_0_same_region();
    let a31 = ka::a3_1_distinct_regions();
    let a32 = ka::a3_2_no_store();
    report(
        "KERNEL ARCH / A3 — read A, write B, read A again: do alias regions save the reload?",
        ITERS_A,
        2.0 * X * ITERS_A as f64,
        Check::Poly,
        vec![
            Row::new("A3-0 one region", "store to B must be assumed to clobber A", move || {
                (a30.f)(ITERS_A, p, obj)
            }),
            Row::new(
                "A3-1 distinct regions",
                "Heap for A, Table for B — can the second read be CSE'd?",
                move || (a31.f)(ITERS_A, p, obj),
            ),
            Row::new("A3-2 no store at all", "the ceiling", move || {
                (a32.f)(ITERS_A, p, obj)
            }),
        ],
    );

    // --- A5: the post-call error poll ---
    let a50 = ka::a5_0_call_poll();
    let a51 = ka::a5_1_inline_poll();
    let a52 = ka::a5_2_no_poll();
    report(
        "KERNEL ARCH / A5 — the error poll emitted after every call",
        ITERS_A,
        X * ITERS_A as f64,
        Check::Poly,
        vec![
            Row::new("A5-0 today", "call __rtsadp_err_pending + branch", move || {
                (a50.f)(ITERS_A, p, obj)
            }),
            Row::new(
                "A5-1 inline load+branch",
                "the Rust `?` shape — read the flag, branch, no call",
                move || (a51.f)(ITERS_A, p, obj),
            ),
            Row::new("A5-2 no poll", "a callee proven non-throwing — the ceiling", move || {
                (a52.f)(ITERS_A, p, obj)
            }),
        ],
    );

    // --- A6: uniform 5-slot thunk vs a native signature ---
    // A HEADER-FREE pair, so the kernel loads at offsets 0 and 8 with no shape
    // word to skip. An earlier version reused the shaped object and read one
    // field twice; the checksum caught it, which is why this pair exists.
    let pair = slab::arena::alloc_raw(&[poly::from_f64(X) as i64, poly::from_f64(Y) as i64]) as i64;
    let a60 = ka::a6_0_uniform_thunk();
    let a61 = ka::a6_1_native_sig();
    report(
        "KERNEL ARCH / A6 — uniform 5-slot thunk ABI vs a native fn(f64,f64)->f64",
        ITERS_A,
        X * Y * ITERS_A as f64,
        Check::Poly,
        vec![
            Row::new(
                "A6-0 uniform thunk",
                "5 tagged slots, boxed args, tagged result",
                move || (a60.f)(ITERS_A, p, pair),
            ),
            Row::new(
                "A6-1 native signature",
                "two f64 in registers, f64 out",
                move || (a61.f)(ITERS_A, p, pair),
            ),
        ],
    );
}
