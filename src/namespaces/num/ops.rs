//! `num` runtime operations.

const OVERFLOW_SENTINEL: i64 = i64::MIN;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_CHECKED_ADD(a: i64, b: i64) -> i64 {
    a.checked_add(b).unwrap_or(OVERFLOW_SENTINEL)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_CHECKED_SUB(a: i64, b: i64) -> i64 {
    a.checked_sub(b).unwrap_or(OVERFLOW_SENTINEL)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_CHECKED_MUL(a: i64, b: i64) -> i64 {
    a.checked_mul(b).unwrap_or(OVERFLOW_SENTINEL)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_CHECKED_DIV(a: i64, b: i64) -> i64 {
    a.checked_div(b).unwrap_or(OVERFLOW_SENTINEL)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_SATURATING_ADD(a: i64, b: i64) -> i64 {
    a.saturating_add(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_SATURATING_SUB(a: i64, b: i64) -> i64 {
    a.saturating_sub(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_SATURATING_MUL(a: i64, b: i64) -> i64 {
    a.saturating_mul(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_ADD(a: i64, b: i64) -> i64 {
    a.wrapping_add(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_SUB(a: i64, b: i64) -> i64 {
    a.wrapping_sub(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_MUL(a: i64, b: i64) -> i64 {
    a.wrapping_mul(b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_NEG(a: i64) -> i64 {
    a.wrapping_neg()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_SHL(a: i64, n: i64) -> i64 {
    a.wrapping_shl(n as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_WRAPPING_SHR(a: i64, n: i64) -> i64 {
    a.wrapping_shr(n as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_COUNT_ONES(a: i64) -> i64 {
    a.count_ones() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_COUNT_ZEROS(a: i64) -> i64 {
    a.count_zeros() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_LEADING_ZEROS(a: i64) -> i64 {
    a.leading_zeros() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_TRAILING_ZEROS(a: i64) -> i64 {
    a.trailing_zeros() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_ROTATE_LEFT(a: i64, n: i64) -> i64 {
    a.rotate_left(n as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_ROTATE_RIGHT(a: i64, n: i64) -> i64 {
    a.rotate_right(n as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_REVERSE_BITS(a: i64) -> i64 {
    a.reverse_bits()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_NUM_SWAP_BYTES(a: i64) -> i64 {
    a.swap_bytes()
}
