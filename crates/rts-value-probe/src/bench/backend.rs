//! Kernel BACKEND driver — Cranelift against LLVM, same machine, same data.
//!
//! The Rust rows here are compiled by LLVM at `opt-level = 3` (this crate sets
//! it explicitly in the workspace manifest, so it does not inherit the shipping
//! profile's `opt-level = "z"`). The Cranelift rows are emitted by
//! `emit/kernel_backend.rs` with the engine's own ISA flags. Both walk the same
//! array and must produce the same checksum, so a mismatch is a bug in the
//! comparison rather than a finding.

use std::hint::black_box;

use crate::emit::{KernelFn, kernel_backend};
use crate::harness::{Check, Row, report};

/// 8192 elements = 64 KB, which sits in L2 on every machine this runs on, so the
/// loop is compute-bound rather than a memory-bandwidth measurement.
const ELEMS: i64 = 8_192;
/// Enough passes for the median to be stable.
const REPS: i64 = 4_000;

/// `[payload_array_ptr, data_base]` — the header shape every compiled kernel
/// expects in its second argument.
struct Data {
    _payload: Vec<i64>,
    _bytes: Vec<i64>,
    hdr: Vec<i64>,
}

impl Data {
    fn from_i64(v: Vec<i64>) -> Self {
        let payload = vec![0i64; 1];
        let hdr = vec![payload.as_ptr() as i64, v.as_ptr() as i64];
        Data {
            _payload: payload,
            _bytes: v,
            hdr,
        }
    }
    fn from_f64(v: Vec<f64>) -> Self {
        // Reinterpret as i64 words so one struct serves both; the kernel loads
        // F64 from the same address either way.
        let as_i64: Vec<i64> = v.iter().map(|x| x.to_bits() as i64).collect();
        Data::from_i64(as_i64)
    }
    fn ptr(&self) -> i64 {
        self.hdr.as_ptr() as i64
    }
}

pub fn kernel_backend_gap() {
    let ints: Vec<i64> = (0..ELEMS).collect();
    let floats: Vec<f64> = (0..ELEMS).map(|i| (i as f64) * 0.5 - 1024.0).collect();

    vec_sum(&ints);
    fp_chain(&floats);
    branchy(&ints);
    fp_reduce(&floats);
    fp_predicated(&floats);
}

/// Run a compiled kernel `REPS` times, returning the last result.
fn drive(f: KernelFn, hdr: i64) -> i64 {
    let mut last = 0i64;
    for _ in 0..REPS {
        last = f(black_box(ELEMS), black_box(hdr), 0);
    }
    last
}

// ---------------------------------------------------------------------------

fn vec_sum(ints: &[i64]) {
    let d = Data::from_i64(ints.to_vec());
    let hdr = d.ptr();
    let k = kernel_backend::vec_sum();
    let f = k.f;
    let kt = kernel_backend::vec_sum_tuned();
    let ft = kt.f;
    let expect: i64 = ints.iter().sum();
    let owned = ints.to_vec();

    report(
        "KERNEL BACKEND / VEC SUM — `s += a[i]`, the loop LLVM autovectorizes",
        ELEMS * REPS,
        expect as f64,
        Check::Int,
        vec![
            Row::new(
                "V-CL Cranelift (engine ISA flags)",
                "one scalar add per element — Cranelift has no loop vectorizer",
                move || drive(f, hdr),
            ),
            Row::new(
                "V-CLT Cranelift, ptr-increment + unrolled 4x",
                "falsifier: is the gap my naive IR, or the missing vectorizer?",
                move || drive(ft, hdr),
            ),
            Row::new(
                "V-LLVM Rust at opt-level=3",
                "LLVM widens the reduction to vector adds",
                move || {
                    let mut last = 0i64;
                    for _ in 0..REPS {
                        let mut s = 0i64;
                        for &x in black_box(&owned).iter() {
                            s += x;
                        }
                        last = s;
                    }
                    last
                },
            ),
        ],
    );
    drop(k);
    drop(kt);
}

fn fp_chain(floats: &[f64]) {
    let d = Data::from_f64(floats.to_vec());
    let hdr = d.ptr();
    let k = kernel_backend::fp_chain();
    let f = k.f;
    let mut s = 0.0f64;
    for &x in floats.iter() {
        s = s * 1.000_000_1 + x;
    }
    let expect = s;
    let owned = floats.to_vec();

    report(
        "KERNEL BACKEND / FP CHAIN — `s = s*k + a[i]`, dependent, no parallelism to find",
        ELEMS * REPS,
        expect,
        Check::Poly,
        vec![
            Row::new(
                "F-CL Cranelift",
                "latency-bound: one fmul + one fadd per element",
                move || drive(f, hdr),
            ),
            Row::new(
                "F-LLVM Rust at opt-level=3",
                "same dependence chain; LLVM cannot reorder FP either",
                move || {
                    let mut last = 0i64;
                    for _ in 0..REPS {
                        let mut s = 0.0f64;
                        for &x in black_box(&owned).iter() {
                            s = s * 1.000_000_1 + x;
                        }
                        last = s.to_bits() as i64;
                    }
                    last
                },
            ),
        ],
    );
    drop(k);
}

fn branchy(ints: &[i64]) {
    let d = Data::from_i64(ints.to_vec());
    let hdr = d.ptr();
    let k = kernel_backend::branchy();
    let f = k.f;
    let mut e = 0i64;
    for &x in ints.iter() {
        if x & 1 != 0 { e += x } else { e -= x }
    }
    let expect = e;
    let owned = ints.to_vec();

    report(
        "KERNEL BACKEND / BRANCHY — `a[i]&1 ? s+=a[i] : s-=a[i]`, branch vs cmov",
        ELEMS * REPS,
        expect as f64,
        Check::Int,
        vec![
            Row::new("B-CL Cranelift", "explicit `select`", move || drive(f, hdr)),
            Row::new(
                "B-LLVM Rust at opt-level=3",
                "LLVM chooses branch or cmov by its own heuristic",
                move || {
                    let mut last = 0i64;
                    for _ in 0..REPS {
                        let mut s = 0i64;
                        for &x in black_box(&owned).iter() {
                            if x & 1 != 0 { s += x } else { s -= x }
                        }
                        last = s;
                    }
                    last
                },
            ),
        ],
    );
    drop(k);
}

fn fp_reduce(floats: &[f64]) {
    let d = Data::from_f64(floats.to_vec());
    let hdr = d.ptr();
    let k = kernel_backend::fp_reduce();
    let f = k.f;
    let mut s = 0.0f64;
    for &x in floats.iter() {
        s += x * x;
    }
    let expect = s;
    let owned = floats.to_vec();

    report(
        "KERNEL BACKEND / FP REDUCE — `s += a[i]*a[i]`, LLVM may NOT reassociate",
        ELEMS * REPS,
        expect,
        Check::Poly,
        vec![
            Row::new("R-CL Cranelift", "scalar fmul + fadd", move || drive(f, hdr)),
            Row::new(
                "R-LLVM Rust at opt-level=3",
                "no fast-math, so LLVM keeps it scalar too — parity expected",
                move || {
                    let mut last = 0i64;
                    for _ in 0..REPS {
                        let mut s = 0.0f64;
                        for &x in black_box(&owned).iter() {
                            s += x * x;
                        }
                        last = s.to_bits() as i64;
                    }
                    last
                },
            ),
        ],
    );
    drop(k);
}

fn fp_predicated(floats: &[f64]) {
    let d = Data::from_f64(floats.to_vec());
    let hdr = d.ptr();
    let k = kernel_backend::fp_predicated();
    let f = k.f;
    let mut s = 0.0f64;
    for &x in floats.iter() {
        s += if x > 0.0 { x } else { 0.0 };
    }
    let expect = s;
    let owned = floats.to_vec();

    report(
        "KERNEL BACKEND / FP PREDICATED — `s += a[i]>0 ? a[i] : 0`, a proven filter+sum",
        ELEMS * REPS,
        expect,
        Check::Poly,
        vec![
            Row::new("P-CL Cranelift", "fcmp + select + fadd", move || drive(f, hdr)),
            Row::new(
                "P-LLVM Rust at opt-level=3",
                "same three ops; the question is scheduling",
                move || {
                    let mut last = 0i64;
                    for _ in 0..REPS {
                        let mut s = 0.0f64;
                        for &x in black_box(&owned).iter() {
                            s += if x > 0.0 { x } else { 0.0 };
                        }
                        last = s.to_bits() as i64;
                    }
                    last
                },
            ),
        ],
    );
    drop(k);
}
