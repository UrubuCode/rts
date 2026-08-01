//! Driver for kernel C — the value representation itself.

use crate::emit;
use crate::harness::{Check, Row, report};
use crate::poly;

use super::{ITERS_C, MASK, N_OBJS};

pub fn kernel_c() {
    let n = N_OBJS * 2;
    let mut boxed: Vec<i64> = Vec::with_capacity(n);
    let mut vals: Vec<f64> = Vec::with_capacity(n);
    let mut tags: Vec<i64> = Vec::with_capacity(n);
    for k in 0..N_OBJS as i64 {
        for v in [k as f64, (k + 1) as f64] {
            boxed.push(poly::from_f64(v) as i64);
            vals.push(v);
            tags.push(emit::kernel_c::TAG_F64);
        }
    }
    let hdr = [
        boxed.as_ptr() as i64,
        vals.as_ptr() as i64,
        tags.as_ptr() as i64,
    ];
    let p = hdr.as_ptr() as i64;

    let expect: f64 = (0..ITERS_C).fold(0.0, |s, i| {
        let k = (i & MASK) as f64;
        s + k * (k + 1.0)
    });

    let c0 = emit::kernel_c::c0_nanbox();
    let c0b = emit::kernel_c::c0b_nanbox_fp_guard();
    let c1 = emit::kernel_c::c1_two_slot();
    let c2 = emit::kernel_c::c2_native();

    report(
        "KERNEL C — number/value representation only (no heap, no calls), 20M iterations",
        ITERS_C,
        expect,
        Check::Poly,
        vec![
            Row::new(
                "C0 NaN-box",
                "1 load/operand, guard = band+icmp vs 64-bit const",
                move || (c0.f)(ITERS_C, p, MASK),
            ),
            Row::new(
                "C0b NaN-box FP guard",
                "1 load/operand into XMM, guard = ucomisd self-compare",
                move || (c0b.f)(ITERS_C, p, MASK),
            ),
            Row::new(
                "C1 two-slot {tag,val}",
                "2 loads/operand, guard = icmp vs small imm",
                move || (c1.f)(ITERS_C, p, MASK),
            ),
            Row::new("C2 native f64", "no tag, no guard — the floor", move || {
                (c2.f)(ITERS_C, p, MASK)
            }),
        ],
    );
    drop((boxed, vals, tags));
}
