//! `math.random_f64` / `math.random_i64_range` / `math.seed` — xorshift64 PRNG.
//!
//! Thread-local state so concurrent code does not need a lock. Default seed
//! is a non-zero constant; callers override via `math.seed` for reproducibility.

use std::cell::Cell;

thread_local! {
    static RNG_STATE: Cell<u64> = const { Cell::new(0x853c_49e6_748f_ea9b) };
}

fn next_u64() -> u64 {
    RNG_STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
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

/// Seeds the PRNG. Zero is replaced by the default seed (xorshift is stuck on 0).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SEED(seed: u64) {
    let s = if seed == 0 { 0x853c_49e6_748f_ea9b } else { seed };
    RNG_STATE.with(|cell| cell.set(s));
}
