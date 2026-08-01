//! Kernel IR LADDER driver — what each defect in the engine's emitted IR costs
//! AFTER the optimizer has had its pass.
//!
//! The `rts ir` dump is pre-optimization, so no cost can be read off it directly.
//! These rows emit that exact IR through the same Cranelift settings the engine
//! uses and let the egraph do whatever it can, then strip one defect per row.
//! The delta between adjacent rows is that defect's real price.

use std::hint::black_box;

use crate::emit::{KernelFn, kernel_ir};
use crate::harness::{Check, Row, report};
use crate::slab::{self, Entry};

const ELEMS: i64 = 8_192;
const REPS: i64 = 500;

/// Keeps the data alive and hands out the `[payload_ptr, data_base]` header the
/// compiled kernels expect in their second argument.
struct Fixture {
    _payload: Vec<i64>,
    _data: Vec<i64>,
    hdr: Vec<i64>,
    handle: u64,
}

fn fixture(values: &[f64]) -> Fixture {
    // The load path reads raw `f64` bits; an ordinary double is already a valid
    // PolyValue (it is not in the boxed quadrant), so the SAME words serve both
    // the load rows and the slab rows.
    let words: Vec<i64> = values.iter().map(|v| v.to_bits() as i64).collect();
    slab::sharded::reset();
    let handle = slab::sharded::alloc(Entry::Vec(Box::new(words.clone())));
    let payload = vec![0i64; 1];
    let hdr = vec![payload.as_ptr() as i64, words.as_ptr() as i64];
    Fixture {
        _payload: payload,
        _data: words,
        hdr,
        handle,
    }
}

impl Fixture {
    fn hdr_ptr(&self) -> i64 {
        self.hdr.as_ptr() as i64
    }
}

fn drive(f: KernelFn, hdr: i64, recv: i64) -> i64 {
    let mut last = 0i64;
    for _ in 0..REPS {
        last = f(black_box(ELEMS), black_box(hdr), black_box(recv));
    }
    last
}

pub fn kernel_ir_ladder() {
    // Non-integral values so the generic add's `number_result` cannot tighten
    // every result to a tagged int32 — the engine's mixed case.
    let values: Vec<f64> = (0..ELEMS).map(|i| (i as f64) * 0.5 - 1024.0).collect();
    sum_ladder(&values);
    double_read(&values);
}

// ---------------------------------------------------------------------------

fn sum_ladder(values: &[f64]) {
    let fx = fixture(values);
    let hdr = fx.hdr_ptr();
    let recv = fx.handle as i64;

    let k0 = kernel_ir::e0_engine();
    let k1 = kernel_ir::e1_int_bound();
    let kb = kernel_ir::e1b_inline_fastpath();
    let k2 = kernel_ir::e2_load();
    let k3 = kernel_ir::e3_proven();
    let (f0, f1, fb_, f2, f3) = (k0.f, k1.f, kb.f, k2.f, k3.f);

    let expect: f64 = values.iter().fold(0.0f64, |a, b| a + b);

    report(
        "KERNEL IR LADDER / SUM — the engine's emitted IR for `s = s + a[i]`, stripped",
        ELEMS * REPS,
        expect,
        Check::Poly,
        vec![
            Row::new(
                "E0 engine IR verbatim",
                "fcvt per iter + array-read CALL + box/NaN-canon + generic-add CALL + tag-check unbox",
                move || drive(f0, hdr, recv),
            ),
            Row::new(
                "E1 - integer loop bound",
                "removes 1 `fcvt_from_sint` per iteration (what `a.length` already gives)",
                move || drive(f1, hdr, recv),
            ),
            Row::new(
                "E1b + inline tag-check fast path on `+` (design doc; NOT emitted today)",
                "read is still a CALL; only the add gets its documented fast path",
                move || drive(fb_, hdr, recv),
            ),
            Row::new(
                "E2 - array read as a `load`",
                "removes the first CALL; the generic add and box/unbox stay",
                move || drive(f2, hdr, recv),
            ),
            Row::new(
                "E3 - proven add (`fadd`)",
                "removes the second CALL and the whole box/unbox round trip",
                move || drive(f3, hdr, recv),
            ),
        ],
    );
    drop((k0, k1, kb, k2, k3));
}

/// `s += a[i] * a[i]` — the same element read twice. With a CALL the two reads
/// are two opaque calls the optimizer may not merge; with a `load` marked
/// `readonly` the egraph should collapse them into one.
fn double_read(values: &[f64]) {
    let fx = fixture(values);
    let hdr = fx.hdr_ptr();
    let recv = fx.handle as i64;

    let kc = kernel_ir::e0b_double_read_call();
    let kl = kernel_ir::e2b_double_read_load();
    let (fc, fl) = (kc.f, kl.f);

    let expect: f64 = values.iter().fold(0.0f64, |a, b| a + b * b);

    report(
        "KERNEL IR LADDER / DOUBLE READ — `s += a[i]*a[i]`: can the optimizer merge the two reads?",
        ELEMS * REPS,
        expect,
        Check::Poly,
        vec![
            Row::new(
                "D0 two array-read CALLS (the engine's shape)",
                "identical args, but a call is opaque — no CSE possible",
                move || drive(fc, hdr, recv),
            ),
            Row::new(
                "D1 two `load`s from the same address",
                "the egraph can prove them equal and keep one",
                move || drive(fl, hdr, recv),
            ),
        ],
    );
    drop((kc, kl));
}
