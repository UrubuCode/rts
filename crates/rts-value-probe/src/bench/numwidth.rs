//! Driver for kernel NUM — which numeric WIDTH wins, and at what.
//!
//! Motivated by a direct question: could RTS swap its `i64`/`i32` paths for
//! `f64`/`f32`? The value model cannot (a JS `number` is an `f64` by spec, a
//! handle is a `u64` that does not survive an `f64` round trip past 2^53), but
//! the engine also moves numbers that are not user values. This prices the
//! choice for those, on the four shapes where the answer differs.
//!
//! ## RESULTS (Ryzen 7 5700G, 8 cores / 16 threads, 512 KB L2/core, 16 MB L3)
//!
//! ```text
//! NUM.1 dependent chain     i64 23.6   i32 23.3   f64 34.4   f32 34.0   ms
//! NUM.2 4 independent       i64 23.1   i32 25.2   f64 41.4   f32 40.1
//! NUM.3 bulk, in L2         i64 16.3   i32 14.0   f64 14.0   f32 13.8
//! NUM.3b bulk, past L3      i64 15.1   i32 14.3   f64 15.0   f32 14.1
//! NUM.4 SIMD, same elems    i64x2 14.6 i32x4 4.7  f64x2 15.1 f32x4 3.6
//! NUM.5 conversion tax      none 24.6             via f64 47.9  via f32 61.9
//! ```
//!
//! Four conclusions, and only the fourth is a place to USE a float:
//!
//! 1. **Scalar integer wins, and f32 is not faster than f64.** 1.46x on the
//!    latency chain, 1.74x on the throughput one — and `f32` matches `f64` in
//!    both, because FP latency is a property of the pipeline, not of the width.
//!    Anywhere RTS does scalar arithmetic on non-user values, `i64` is correct.
//! 2. **Halving the bytes is worth ~6%, not 2x.** Even with a 64 MB working set
//!    past L3, a loop consuming ONE element per iteration is issue-bound rather
//!    than bandwidth-bound (~0.75 ns/elem is nowhere near DRAM peak). "f32 uses
//!    half the memory" is true and nearly irrelevant at this access shape.
//! 3. **Crossing domains costs 2x, and f32 costs MORE than f64** (2.52x vs
//!    1.95x): `fcvt` to/from a 32-bit float is not cheaper. A mixed
//!    representation pays this on every boundary, which is the argument for
//!    picking one domain per data structure and staying in it.
//! 4. **SIMD is the one real win, and it is about LANES, not bytes.** 4x32
//!    against 2x64 is 3.1x (`i32x4` vs `i64x2`) and 4.2x (`f32x4` vs `f64x2`) on
//!    the same element count. This is the shape where a narrow type pays — and
//!    it is exactly `Int32Array`/`Float32Array` bulk work, where the width is
//!    already fixed BY THE SPEC and so no JS/TS semantics are at stake.

use cranelift_codegen::ir::types;

use crate::emit::kernel_num;
use crate::harness::{Check, Row, report};

/// Ring of elements, in ELEMENTS not bytes — each width gets its own buffer of
/// this many, so every variant walks the same number of elements and the byte
/// traffic is the thing that differs.
///
/// 64 Ki elements is 512 KB at 8 bytes and 256 KB at 4 — both inside L2 on the
/// measuring box (Ryzen 7 5700G, 512 KB L2/core, 16 MB L3). That is deliberate:
/// it isolates the LOAD from the memory system, so NUM.3 is the cache-resident
/// case and NUM.3b below is the one that actually tests bandwidth.
const N_ELEMS: usize = 1 << 16;
const ELEM_MASK: i64 = (N_ELEMS as i64) - 1;

/// 8 Mi elements: 64 MB at 8 bytes/elem, 32 MB at 4 — both past this box's
/// 16 MB L3, so the loop is served by DRAM and the byte width is the variable
/// that matters. Without this pair the "half the bytes" claim cannot be tested
/// at all, only assumed.
const N_BIG: usize = 1 << 23;
const BIG_MASK: i64 = (N_BIG as i64) - 1;

const ITERS_SCALAR: i64 = 50_000_000;
const ITERS_BULK: i64 = 20_000_000;
/// The SIMD rows consume `lane_count` elements per iteration, so they run
/// proportionally fewer iterations for the same element count.
const ITERS_SIMD_X2: i64 = ITERS_BULK / 2;
const ITERS_SIMD_X4: i64 = ITERS_BULK / 4;

/// 16-byte-aligned storage, so a vector load is aligned for every lane width.
#[repr(align(16))]
struct Aligned<T>(Vec<T>);

fn buffers(n: usize) -> (Aligned<i64>, Aligned<i32>, Aligned<f64>, Aligned<f32>) {
    let i64s: Vec<i64> = (0..n).map(|k| (k % 97) as i64).collect();
    let i32s: Vec<i32> = (0..n).map(|k| (k % 97) as i32).collect();
    let f64s: Vec<f64> = (0..n).map(|k| (k % 97) as f64).collect();
    let f32s: Vec<f32> = (0..n).map(|k| (k % 97) as f32).collect();
    (Aligned(i64s), Aligned(i32s), Aligned(f64s), Aligned(f32s))
}

pub fn kernel_num_width() {
    let (bi64, bi32, bf64, bf32) = buffers(N_ELEMS);
    let (pi64, pi32, pf64, pf32) = (
        bi64.0.as_ptr() as i64,
        bi32.0.as_ptr() as i64,
        bf64.0.as_ptr() as i64,
        bf32.0.as_ptr() as i64,
    );

    // ---- shape 1: dependent chain (LATENCY) -------------------------------
    let c_i64 = kernel_num::scalar_chain("num_chain_i64", types::I64, 1);
    let c_i32 = kernel_num::scalar_chain("num_chain_i32", types::I32, 1);
    let c_f64 = kernel_num::scalar_chain("num_chain_f64", types::F64, 1);
    let c_f32 = kernel_num::scalar_chain("num_chain_f32", types::F32, 1);
    report(
        "NUM.1 — dependent chain: s = s + i, one accumulator (LATENCY-bound)",
        ITERS_SCALAR,
        0.0,
        Check::None,
        vec![
            Row::new("i64", "iadd, 1-cycle latency", move || {
                (c_i64.f)(ITERS_SCALAR, 0, 0)
            }),
            Row::new("i32", "iadd on 32-bit", move || (c_i32.f)(ITERS_SCALAR, 0, 0)),
            Row::new("f64", "fadd, 3-4 cycle latency", move || {
                (c_f64.f)(ITERS_SCALAR, 0, 0)
            }),
            Row::new("f32", "fadd, same latency as f64", move || {
                (c_f32.f)(ITERS_SCALAR, 0, 0)
            }),
        ],
    );

    // ---- shape 2: independent chains (THROUGHPUT) --------------------------
    let d_i64 = kernel_num::scalar_chain("num_indep_i64", types::I64, 4);
    let d_i32 = kernel_num::scalar_chain("num_indep_i32", types::I32, 4);
    let d_f64 = kernel_num::scalar_chain("num_indep_f64", types::F64, 4);
    let d_f32 = kernel_num::scalar_chain("num_indep_f32", types::F32, 4);
    report(
        "NUM.2 — four independent accumulators (THROUGHPUT-bound)",
        ITERS_SCALAR,
        0.0,
        Check::None,
        vec![
            Row::new("i64", "4 chains, integer ports", move || {
                (d_i64.f)(ITERS_SCALAR, 0, 0)
            }),
            Row::new("i32", "4 chains, 32-bit", move || {
                (d_i32.f)(ITERS_SCALAR, 0, 0)
            }),
            Row::new("f64", "4 chains, FP ports", move || {
                (d_f64.f)(ITERS_SCALAR, 0, 0)
            }),
            Row::new("f32", "4 chains, FP ports", move || {
                (d_f32.f)(ITERS_SCALAR, 0, 0)
            }),
        ],
    );

    // ---- shape 3: bulk element sum (MEMORY BANDWIDTH) ----------------------
    let b8i = kernel_num::bulk_sum("num_bulk_i64", types::I64);
    let b4i = kernel_num::bulk_sum("num_bulk_i32", types::I32);
    let b8f = kernel_num::bulk_sum("num_bulk_f64", types::F64);
    let b4f = kernel_num::bulk_sum("num_bulk_f32", types::F32);
    report(
        "NUM.3 — bulk sum over an element array (BANDWIDTH: bytes per element)",
        ITERS_BULK,
        0.0,
        Check::None,
        vec![
            Row::new("i64[]", "8 bytes/elem", move || {
                (b8i.f)(ITERS_BULK, pi64, ELEM_MASK)
            }),
            Row::new("i32[]", "4 bytes/elem + sextend", move || {
                (b4i.f)(ITERS_BULK, pi32, ELEM_MASK)
            }),
            Row::new("f64[]", "8 bytes/elem", move || {
                (b8f.f)(ITERS_BULK, pf64, ELEM_MASK)
            }),
            Row::new("f32[]", "4 bytes/elem + fpromote", move || {
                (b4f.f)(ITERS_BULK, pf32, ELEM_MASK)
            }),
        ],
    );

    // ---- shape 3b: the same sum, OUT OF CACHE (real bandwidth) -------------
    {
        let (gi64, gi32, gf64, gf32) = buffers(N_BIG);
        let (qi64, qi32, qf64, qf32) = (
            gi64.0.as_ptr() as i64,
            gi32.0.as_ptr() as i64,
            gf64.0.as_ptr() as i64,
            gf32.0.as_ptr() as i64,
        );
        let g8i = kernel_num::bulk_sum("num_big_i64", types::I64);
        let g4i = kernel_num::bulk_sum("num_big_i32", types::I32);
        let g8f = kernel_num::bulk_sum("num_big_f64", types::F64);
        let g4f = kernel_num::bulk_sum("num_big_f32", types::F32);
        report(
            "NUM.3b — same sum over 8Mi elements, PAST L3 (DRAM bandwidth)",
            ITERS_BULK,
            0.0,
            Check::None,
            vec![
                Row::new("i64[]", "64 MB working set", move || {
                    (g8i.f)(ITERS_BULK, qi64, BIG_MASK)
                }),
                Row::new("i32[]", "32 MB working set", move || {
                    (g4i.f)(ITERS_BULK, qi32, BIG_MASK)
                }),
                Row::new("f64[]", "64 MB working set", move || {
                    (g8f.f)(ITERS_BULK, qf64, BIG_MASK)
                }),
                Row::new("f32[]", "32 MB working set", move || {
                    (g4f.f)(ITERS_BULK, qf32, BIG_MASK)
                }),
            ],
        );
        drop((gi64, gi32, gf64, gf32));
    }

    // ---- shape 4: SIMD (LANES PER VECTOR) ----------------------------------
    let s_i64 = kernel_num::simd_sum("num_simd_i64x2", types::I64X2);
    let s_i32 = kernel_num::simd_sum("num_simd_i32x4", types::I32X4);
    let s_f64 = kernel_num::simd_sum("num_simd_f64x2", types::F64X2);
    let s_f32 = kernel_num::simd_sum("num_simd_f32x4", types::F32X4);
    report(
        "NUM.4 — SIMD sum, same element count (LANES: 2x64 vs 4x32 per vector)",
        ITERS_BULK,
        0.0,
        Check::None,
        vec![
            Row::new("i64x2", "2 lanes/iter", move || {
                (s_i64.f)(ITERS_SIMD_X2, pi64, (ELEM_MASK / 2) - 1)
            }),
            Row::new("i32x4", "4 lanes/iter", move || {
                (s_i32.f)(ITERS_SIMD_X4, pi32, (ELEM_MASK / 4) - 1)
            }),
            Row::new("f64x2", "2 lanes/iter", move || {
                (s_f64.f)(ITERS_SIMD_X2, pf64, (ELEM_MASK / 2) - 1)
            }),
            Row::new("f32x4", "4 lanes/iter", move || {
                (s_f32.f)(ITERS_SIMD_X4, pf32, (ELEM_MASK / 4) - 1)
            }),
        ],
    );

    // ---- shape 5: domain crossing (CONVERSION TAX) -------------------------
    let x64 = kernel_num::conv_roundtrip("num_conv_f64", types::F64);
    let x32 = kernel_num::conv_roundtrip("num_conv_f32", types::F32);
    let base = kernel_num::scalar_chain("num_conv_base", types::I64, 1);
    report(
        "NUM.5 — int -> float -> int per iteration (the CONVERSION tax)",
        ITERS_SCALAR,
        0.0,
        Check::None,
        vec![
            Row::new("no conversion", "iadd only, the floor", move || {
                (base.f)(ITERS_SCALAR, 0, 0)
            }),
            Row::new("via f64", "fcvt_from_sint + fcvt_to_sint_sat", move || {
                (x64.f)(ITERS_SCALAR, 0, 0)
            }),
            Row::new("via f32", "same, 32-bit float", move || {
                (x32.f)(ITERS_SCALAR, 0, 0)
            }),
        ],
    );

    // Keep the buffers alive until every kernel has run.
    drop((bi64, bi32, bf64, bf32));
}
