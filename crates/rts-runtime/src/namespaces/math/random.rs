//! `math.random_f64` / `math.random_i64_range` / `math.seed` — xorshift64 PRNG.
//!
//! Estado por-thread (`thread_local!`) para evitar data race em workloads
//! paralelos (`parallel.*` roda em rayon thread pool). Antes era um
//! `static mut` global — UB sob multi-thread.
//!
//! Custo: a intrinsic inline antiga (que emitia o passo xorshift direto
//! em IR Cranelift via `__RTS_DATA_NS_MATH_RNG_STATE`) foi removida — as
//! chamadas agora vao via `extern "C"`. O simbolo data global tambem foi
//! removido pois `thread_local!` nao tem endereco linker estavel.

use std::cell::Cell;

thread_local! {
    static RNG_STATE: Cell<u64> = const { Cell::new(0x853c_49e6_748f_ea9b) };
}

#[inline(always)]
fn next_u64() -> u64 {
    RNG_STATE.with(|c| {
        let mut x = c.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        c.set(x);
        x
    })
}

/// Uniformly distributed f64 in `[0, 1)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_RANDOM_F64() -> f64 {
    // Top 53 bits — exact f64 mantissa, no bias.
    let bits = next_u64() >> 11;
    bits as f64 / ((1u64 << 53) as f64)
}

/// Uniform i64 in `[lo, hi)`. If `lo >= hi`, returns `lo`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_RANDOM_I64_RANGE(lo: i64, hi: i64) -> i64 {
    if lo >= hi {
        return lo;
    }
    let span = (hi as i128) - (lo as i128);
    let r = next_u64() as i128;
    let offset = r.rem_euclid(span);
    (lo as i128 + offset) as i64
}

/// Seeds the PRNG (current thread).
/// Zero is replaced by the default seed (xorshift is stuck on 0).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SEED(seed: u64) {
    let s = if seed == 0 {
        0x853c_49e6_748f_ea9b
    } else {
        seed
    };
    RNG_STATE.with(|c| c.set(s));
}
