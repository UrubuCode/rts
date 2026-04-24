//! `math.random` / `math.seed` — xorshift64 PRNG.
//!
//! Keeps state thread-local so concurrent code does not need a lock. Seed
//! defaults to a non-zero constant; callers may override via `math.seed`
//! for reproducibility.

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

/// Returns a uniformly distributed f64 in `[0, 1)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_RANDOM() -> f64 {
    // Take the top 53 bits so we fit an exact f64 mantissa without bias.
    let bits = next_u64() >> 11;
    bits as f64 / ((1u64 << 53) as f64)
}

/// Seeds the PRNG. Zero is replaced by the default seed (xorshift is stuck
/// on 0).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_MATH_SEED(seed: u64) {
    let s = if seed == 0 { 0x853c_49e6_748f_ea9b } else { seed };
    RNG_STATE.with(|cell| cell.set(s));
}
