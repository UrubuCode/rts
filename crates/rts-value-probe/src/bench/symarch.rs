//! Kernel SYM driver — how much work must a precompiled symbol do before its
//! call pays for itself?
//!
//! `k = 1` is today's engine: one call per element. `k = 8192` is one call for
//! the whole loop. The inline row is Cranelift generating the same work with no
//! call at all. Every row sums the same array and must produce the same value.

use std::hint::black_box;

use crate::emit::{KernelFn, kernel_sym};
use crate::harness::{Check, Row, report};
use crate::slab::{self, Entry};

const ELEMS: i64 = 8_192;
const REPS: i64 = 500;

fn drive(f: KernelFn, hdr: i64, k: i64) -> i64 {
    let mut last = 0i64;
    for _ in 0..REPS {
        last = f(black_box(ELEMS), black_box(hdr), black_box(k));
    }
    last
}

pub fn kernel_sym_arch() {
    let values: Vec<f64> = (0..ELEMS).map(|i| (i as f64) * 0.5 - 1024.0).collect();
    let words: Vec<i64> = values.iter().map(|v| v.to_bits() as i64).collect();
    slab::sharded::reset();
    let handle = slab::sharded::alloc(Entry::Vec(Box::new(words.clone())));
    let hdr = vec![handle as i64, words.as_ptr() as i64];
    let h = hdr.as_ptr() as i64;

    let expect: f64 = values.iter().fold(0.0f64, |a, b| a + b);

    let kl = kernel_sym::s_chunk_locked();
    let kr = kernel_sym::s_chunk_raw();
    let ki = kernel_sym::s_inline();
    let (fl, fr, fi) = (kl.f, kr.f, ki.f);

    // Chunk sizes double: the crossover, if there is one, shows as the row where
    // the call architecture passes the inline row.
    report(
        "KERNEL SYM / SUPERINSTRUCTION — precompiled native symbol vs Cranelift emitting it",
        ELEMS * REPS,
        expect,
        Check::Poly,
        vec![
            Row::new(
                "K1 call per element (today's engine)",
                "1 element per call, one shard lock per call",
                move || drive(fl, h, 1),
            ),
            Row::new(
                "K4 4 elements per call",
                "call + lock amortized over 4",
                move || drive(fl, h, 4),
            ),
            Row::new(
                "K16 16 elements per call",
                "call + lock amortized over 16",
                move || drive(fl, h, 16),
            ),
            Row::new(
                "K64 64 elements per call",
                "call + lock amortized over 64",
                move || drive(fl, h, 64),
            ),
            Row::new(
                "KALL 1 call for the WHOLE loop",
                "the maximum the architecture can amortize: one call, one lock",
                move || drive(fl, h, ELEMS),
            ),
            Row::new(
                "KRAW 1 call, raw pointer, NO container",
                "the ceiling: superinstruction with the slab removed entirely",
                move || drive(fr, h, ELEMS),
            ),
            Row::new(
                "KIN Cranelift emits the loop (load + fadd, no call)",
                "what the architecture proposes to replace",
                move || drive(fi, h, 1),
            ),
        ],
    );
    drop((kl, kr, ki));
    black_box(&words);
}
