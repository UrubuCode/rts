//! [`PolyValue`](super::PolyValue) bit-layout constants + the `encode` helper.
//!
//! Split out of `value/mod.rs` to keep that file under the 500-line module rule.
//! These are re-exported from the `value` module (`pub use layout::*`), so every
//! call site (`crate::value::BOX_BASE`, `super::encode`, …) resolves unchanged.
//! See the module docs in `value/mod.rs` for the full NaN-boxing rationale.

// ===========================================================================
// Bit-layout constants
// ===========================================================================

/// The negative-quiet-NaN base: sign=1, exponent=0x7FF (all ones), qNaN bit
/// (bit 51)=1. Equivalently: the top 13 bits (63..=51) are all set.
///
/// A word `w` is boxed iff `(w & BOX_BASE) == BOX_BASE`.
pub const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;

/// Bit position of the 3-bit tag inside a boxed word (bits 50..=48).
pub const TAG_SHIFT: u64 = 48;

/// Mask for the 3-bit tag once shifted down.
pub const TAG_MASK: u64 = 0x7;

/// Mask for the 48-bit payload (bits 47..=0).
pub const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// The canonical *positive* quiet NaN. Every NaN fed to [`PolyValue::from_f64`]
/// is normalized to this pattern so that no double ever collides with the
/// negative-qNaN boxed space. It is itself a perfectly valid (and double-classified)
/// NaN — `(CANONICAL_NAN & BOX_BASE) != BOX_BASE` because its sign bit is 0.
///
/// [`PolyValue::from_f64`]: super::PolyValue::from_f64
pub const CANONICAL_NAN: u64 = 0x7FF8_0000_0000_0000;

// ---------------------------------------------------------------------------
// Tags (3 bits → 8 kinds; only 1..=5 are used today, 0/6/7 reserved).
// ---------------------------------------------------------------------------

/// Reserved tag 0 — not used. (Avoid: an all-zero payload with tag 0 would
/// collide with no double, but we keep it free for a future kind to avoid
/// accidental "looks like a small negative qNaN" overlaps in debugging.)
pub const TAG_RESERVED0: u64 = 0;

/// `int32` — payload's low 32 bits hold the `i32` (decode sign-extends from 32).
/// `typeof` ⇒ `"number"`.
pub const TAG_INT32: u64 = 1;

/// Singleton — payload selects a fixed value (see the `SINGLETON_*` consts).
pub const TAG_SINGLETON: u64 = 2;

/// String — payload is a 48-bit string-handle slot index. `typeof` ⇒ `"string"`.
pub const TAG_STR: u64 = 3;

/// Object/array/registered class instance — payload is a 48-bit handle slot.
/// `typeof` ⇒ `"object"`.
pub const TAG_OBJECT: u64 = 4;

/// Function — payload is a 48-bit function-handle slot. `typeof` ⇒ `"function"`.
pub const TAG_FUNCTION: u64 = 5;

/// Reserved tag 6 — future `symbol`.
pub const TAG_RESERVED_SYMBOL: u64 = 6;

/// Reserved tag 7 — future `bigint`.
pub const TAG_RESERVED_BIGINT: u64 = 7;

// ---------------------------------------------------------------------------
// Singleton payload selectors (under TAG_SINGLETON).
// ---------------------------------------------------------------------------

/// `undefined`.
pub const SINGLETON_UNDEFINED: u64 = 0;
/// `null`.
pub const SINGLETON_NULL: u64 = 1;
/// `false`.
pub const SINGLETON_FALSE: u64 = 2;
/// `true`.
pub const SINGLETON_TRUE: u64 = 3;
/// Array hole (elision in a sparse array literal: `[1, , 3]`).
pub const SINGLETON_HOLE: u64 = 4;
/// Internal "no value" sentinel (e.g. an empty slot / uninitialized binding).
/// Never user-observable.
pub const SINGLETON_EMPTY: u64 = 5;

/// Compute the full word for a boxed value from a tag and payload.
///
/// `encode(tag, payload) = BOX_BASE | (tag << 48) | (payload & 0xFFFF_FFFF_FFFF)`.
#[inline(always)]
pub const fn encode(tag: u64, payload: u64) -> u64 {
    BOX_BASE | ((tag & TAG_MASK) << TAG_SHIFT) | (payload & PAYLOAD_MASK)
}
