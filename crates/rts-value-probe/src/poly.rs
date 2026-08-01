//! The PolyValue bit layout under test, copied verbatim from
//! `rts-runtime/src/adapters/value/layout.rs` + `rts-natives/src/heap/poly.rs`.
//!
//! Copied, not imported, on purpose: the probe must not link the runtime, so a
//! number it produces cannot be blamed on unrelated runtime work. The
//! `layout_matches_runtime` test below pins the constants to the documented
//! values, so a drift in the real layout shows up as a probe test failure.

/// Boxed iff the top 13 bits are all ones (negative quiet-NaN quadrant).
pub const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
/// Bits 47..0 — the payload (handle slot index / int32 / singleton selector).
pub const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
/// Bits 50..48 — the 3-bit tag.
pub const TAG_SHIFT: u64 = 48;

pub const TAG_INT32: u64 = 1;
/// Kept for layout completeness — the probe's kernels only ever produce doubles
/// and tagged int32s, but the tag numbering must match the real one to stay a
/// faithful copy.
#[allow(dead_code)]
pub const TAG_OBJECT: u64 = 4;

/// `BOX_BASE | tag<<48 | payload`.
#[inline]
pub const fn encode(tag: u64, payload: u64) -> u64 {
    BOX_BASE | (tag << TAG_SHIFT) | (payload & PAYLOAD_MASK)
}

/// A real double rides the word verbatim — this is the whole point of the
/// scheme and the reason unboxing a proven-double field is a `bitcast`, free.
#[inline]
pub fn from_f64(v: f64) -> u64 {
    if v.is_nan() { 0x7FF8_0000_0000_0000 } else { v.to_bits() }
}

#[inline]
pub fn as_f64(bits: u64) -> f64 {
    f64::from_bits(bits)
}

#[inline]
pub fn is_boxed(bits: u64) -> bool {
    (bits & BOX_BASE) == BOX_BASE
}

/// `genops::number_result` — re-tighten to a tagged int32 when the double is
/// exactly an in-range integer, else keep it a double. Replicated because it is
/// on the hot path of every `__rtsadp_*` arithmetic trampoline.
#[inline]
pub fn number_result(v: f64) -> u64 {
    if v.fract() == 0.0 && v >= i32::MIN as f64 && v <= i32::MAX as f64 && v.is_finite() {
        encode(TAG_INT32, v as i32 as u32 as u64)
    } else {
        from_f64(v)
    }
}

pub const TAG_SINGLETON: u64 = 2;
pub const SINGLETON_FALSE: u64 = 2;
pub const SINGLETON_TRUE: u64 = 3;

/// `PolyValue::bool` — the singleton word a comparison trampoline returns.
#[inline]
pub fn bool_word(b: bool) -> u64 {
    encode(
        TAG_SINGLETON,
        if b { SINGLETON_TRUE } else { SINGLETON_FALSE },
    )
}

/// Is this word a number (inline double or tagged int32)?
#[inline]
pub fn is_number(bits: u64) -> bool {
    !is_boxed(bits) || ((bits >> TAG_SHIFT) & 0x7) == TAG_INT32
}

/// `genops::to_number` for the two kinds this probe's kernels produce.
#[inline]
pub fn to_number(bits: u64) -> f64 {
    if !is_boxed(bits) {
        return as_f64(bits);
    }
    let tag = (bits >> TAG_SHIFT) & 0x7;
    if tag == TAG_INT32 {
        (bits & 0xFFFF_FFFF) as u32 as i32 as f64
    } else {
        f64::NAN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_runtime() {
        assert_eq!(BOX_BASE, 0xFFF8_0000_0000_0000);
        assert_eq!(BOX_BASE >> 51, 0x1FFF, "13 leading ones");
        assert_eq!(PAYLOAD_MASK, (1u64 << 48) - 1);
        assert_eq!(TAG_SHIFT, 48);
    }

    #[test]
    fn doubles_round_trip_and_stay_unboxed() {
        for v in [0.0f64, -0.0, 1.5, -1.5, f64::MIN_POSITIVE, 1e300, -1e300] {
            let w = from_f64(v);
            assert!(!is_boxed(w), "{v} collided with the boxed space");
            assert_eq!(as_f64(w).to_bits(), v.to_bits());
        }
        // -Infinity has bit 51 = 0, so it is NOT in the boxed space.
        assert!(!is_boxed(from_f64(f64::NEG_INFINITY)));
    }

    #[test]
    fn tagged_int32_is_boxed() {
        let w = encode(TAG_INT32, 42);
        assert!(is_boxed(w));
        assert_eq!(to_number(w), 42.0);
    }
}
