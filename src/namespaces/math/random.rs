//! `math.random_f64` / `math.random_i64_range` / `math.seed` — xorshift64 PRNG.
//!
//! State lives in a single global `__RTS_DATA_NS_MATH_RNG_STATE` so codegen
//! can emit the xorshift step inline at the call site (intrinsic path) and
//! the extern shim can share the same backing store. Single-threaded by
//! construction; multithreaded workloads should seed separate state.

/// Global PRNG state. Exported with the `__RTS_DATA_*` convention so
/// Cranelift can reference it via `declare_data` when inlining the
/// RandomF64 intrinsic.
#[unsafe(no_mangle)]
pub static mut __RTS_DATA_NS_MATH_RNG_STATE: u64 = 0x853c_49e6_748f_ea9b;

#[inline(always)]
fn next_u64() -> u64 {
    // SAFETY: single-threaded access. Callers who need multithreaded RNG
    // must provide their own state (future API).
    unsafe {
        let mut x = __RTS_DATA_NS_MATH_RNG_STATE;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        __RTS_DATA_NS_MATH_RNG_STATE = x;
        x
    }
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
    // SAFETY: single-threaded.
    unsafe {
        __RTS_DATA_NS_MATH_RNG_STATE = s;
    }
}
