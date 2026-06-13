//! # `value` — the one true runtime value: [`PolyValue`] (NaN-boxed 64-bit word)
//!
//! Every polymorphic / `any` / container-element / union JS value in the new
//! engine is a single 64-bit word: a [`PolyValue`]. There is no side-table of
//! "what does this `i64` slot really mean" — the tag lives **inside the value**.
//! `typeof` is a tag inspection; box/unbox are single pure Cranelift ops the
//! egraph can constant-fold.
//!
//! Real `f64` doubles round-trip **exactly** through this representation (every
//! finite double, both zeroes, both infinities, subnormals — and even NaN,
//! which is canonicalized; see below). Heap references are **HandleTable slot
//! indices** (48-bit), never raw pointers, so a boxed handle is GC-safe by
//! construction: the GC sees the slot, the slot owns the pointer.
//!
//! ## NaN-boxing in the *negative* quiet-NaN space
//!
//! IEEE-754 `binary64` (`f64`) bit layout:
//!
//! ```text
//!  63  62        52 51                                                  0
//! ┌───┬───────────┬─────────────────────────────────────────────────────┐
//! │ S │  exponent │                     mantissa (52)                     │
//! └───┴───────────┴─────────────────────────────────────────────────────┘
//! ```
//!
//! A value is **NaN** iff every exponent bit is 1 and the mantissa is nonzero.
//! A **quiet** NaN additionally sets the top mantissa bit (bit 51). There is a
//! whole `2^51`-sized space of NaN bit-patterns that no normal arithmetic
//! produces meaningfully; we steal the **negative** quiet-NaN slice for boxing:
//!
//! ```text
//! BOX_BASE = 0xFFF8_0000_0000_0000
//!            └┬┘└──┬──┘ └┘
//!  sign=1 ────┘    │      └─ qNaN bit (bit 51) = 1
//!  exp = 0x7FF ────┘
//!  → the top 13 bits (63..=51) are ALL ONES.
//! ```
//!
//! A 64-bit word is a **BOXED** (non-double) value iff `(bits & BOX_BASE) ==
//! BOX_BASE`, i.e. all 13 top bits are set. Otherwise it is an **INLINE
//! DOUBLE**: interpret the bits directly as `f64`.
//!
//! When boxed, bits `50..=0` (51 bits) carry our data:
//!
//! ```text
//!  63        51 50  48 47                                              0
//! ┌────────────┬──────┬──────────────────────────────────────────────────┐
//! │ 1111111111111 TAG │                  PAYLOAD (48)                      │
//! └────────────┴──────┴──────────────────────────────────────────────────┘
//!   13 ones      3 bits   48-bit handle slot / int32 / singleton selector
//! ```
//!
//! - **TAG** = bits `50..=48` (3 bits → 8 kinds).
//! - **PAYLOAD** = bits `47..=0` (48 bits).
//!
//! 48 bits is exactly the HandleTable slot width (16-bit generation + 48-bit
//! slot in the runtime `u64` handle — here we box only the 48-bit slot index),
//! and comfortably holds a full `i32`.
//!
//! ## Disjointness: NaN canonicalization
//!
//! For the `(bits & BOX_BASE) == BOX_BASE` discriminator to be sound, **no real
//! double may ever land in the negative-qNaN boxed space**. The only doubles
//! whose top 13 bits are all-ones are *negative quiet NaNs* — so in
//! [`PolyValue::from_f64`] we **canonicalize** any NaN input to the single
//! *positive* canonical quiet NaN `0x7FF8_0000_0000_0000` (top bit = sign = 0).
//! That pattern is `is_double` (it is a NaN, `as_f64().is_nan()` holds) yet is
//! NOT in the boxed space, so the two universes stay disjoint.
//!
//! Note `-Infinity` = `0xFFF0_0000_0000_0000` has bit 51 = **0**, so it is *not*
//! in the boxed space and round-trips as an ordinary double. (Tested.)
//!
//! ## Cranelift emit helpers
//!
//! The bottom of this module proves box/unbox are pure straight-line IR (no
//! extern calls): `band`/`bor`/`icmp`/`bitcast`/`ishl`/`sshr`. The JIT roundtrip
//! test compiles one of them and checks it against the pure-Rust model.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, InstBuilder, MemFlags, Value};
use cranelift_frontend::FunctionBuilder;

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
const fn encode(tag: u64, payload: u64) -> u64 {
    BOX_BASE | ((tag & TAG_MASK) << TAG_SHIFT) | (payload & PAYLOAD_MASK)
}

// ===========================================================================
// PolyValue
// ===========================================================================

/// A NaN-boxed 64-bit JS value. See the module docs for the full bit layout.
///
/// Copy/POD: it is just a `u64`. Equality is bitwise (`Eq`), which is the
/// correct notion of *identical representation* — NOT JS `===` (e.g. `NaN !==
/// NaN` in JS but two canonical NaNs are bit-equal here; and `+0`/`-0` are JS-`===`
/// but bit-distinct here). Use the dedicated comparison routines in higher
/// layers for JS semantics.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PolyValue(pub u64);

impl PolyValue {
    // ---- raw access ----

    /// The raw 64-bit word.
    #[inline(always)]
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Wrap a raw 64-bit word (no validation).
    #[inline(always)]
    pub const fn from_raw(bits: u64) -> Self {
        PolyValue(bits)
    }

    // ---- constructors ----

    /// Box an `f64`. NaN inputs are canonicalized to [`CANONICAL_NAN`] so they
    /// never collide with the boxed space; every other double is stored bit-for-bit.
    #[inline(always)]
    pub fn from_f64(f: f64) -> Self {
        if f.is_nan() {
            PolyValue(CANONICAL_NAN)
        } else {
            PolyValue(f.to_bits())
        }
    }

    /// Box a small integer as a tagged `int32`. `typeof` ⇒ `"number"`.
    #[inline(always)]
    pub fn from_i32(i: i32) -> Self {
        // Store the i32 in the low 32 bits of the payload (zero-extended; the
        // decode sign-extends back). The high 16 payload bits are 0.
        PolyValue(encode(TAG_INT32, i as u32 as u64))
    }

    /// Box a 48-bit string handle slot. `typeof` ⇒ `"string"`.
    #[inline(always)]
    pub fn from_str_handle(slot: u64) -> Self {
        debug_assert!(slot <= PAYLOAD_MASK, "string handle slot exceeds 48 bits");
        PolyValue(encode(TAG_STR, slot))
    }

    /// Box a 48-bit object handle slot. `typeof` ⇒ `"object"`.
    #[inline(always)]
    pub fn from_object_handle(slot: u64) -> Self {
        debug_assert!(slot <= PAYLOAD_MASK, "object handle slot exceeds 48 bits");
        PolyValue(encode(TAG_OBJECT, slot))
    }

    /// Box a 48-bit function handle slot. `typeof` ⇒ `"function"`.
    #[inline(always)]
    pub fn from_function_handle(slot: u64) -> Self {
        debug_assert!(slot <= PAYLOAD_MASK, "function handle slot exceeds 48 bits");
        PolyValue(encode(TAG_FUNCTION, slot))
    }

    /// `undefined`.
    #[inline(always)]
    pub const fn undefined() -> Self {
        PolyValue(encode(TAG_SINGLETON, SINGLETON_UNDEFINED))
    }

    /// `null`.
    #[inline(always)]
    pub const fn null() -> Self {
        PolyValue(encode(TAG_SINGLETON, SINGLETON_NULL))
    }

    /// `true` / `false`.
    #[inline(always)]
    pub const fn bool(b: bool) -> Self {
        let sel = if b { SINGLETON_TRUE } else { SINGLETON_FALSE };
        PolyValue(encode(TAG_SINGLETON, sel))
    }

    /// The array hole singleton.
    #[inline(always)]
    pub const fn hole() -> Self {
        PolyValue(encode(TAG_SINGLETON, SINGLETON_HOLE))
    }

    /// The internal "no value" singleton.
    #[inline(always)]
    pub const fn empty() -> Self {
        PolyValue(encode(TAG_SINGLETON, SINGLETON_EMPTY))
    }

    // ---- low-level tag/payload ----

    /// True iff this word is in the boxed (non-double) space.
    #[inline(always)]
    pub const fn is_boxed(&self) -> bool {
        (self.0 & BOX_BASE) == BOX_BASE
    }

    /// The 3-bit tag. **Only meaningful when [`is_boxed`](Self::is_boxed).**
    #[inline(always)]
    pub const fn tag(&self) -> u64 {
        (self.0 >> TAG_SHIFT) & TAG_MASK
    }

    /// The 48-bit payload. **Only meaningful when boxed.**
    #[inline(always)]
    const fn payload(&self) -> u64 {
        self.0 & PAYLOAD_MASK
    }

    // ---- predicates ----

    /// True iff this is an inline `f64` (i.e. not boxed).
    #[inline(always)]
    pub const fn is_double(&self) -> bool {
        !self.is_boxed()
    }

    /// True iff this is a tagged `int32`.
    #[inline(always)]
    pub const fn is_int32(&self) -> bool {
        self.is_boxed() && self.tag() == TAG_INT32
    }

    /// True iff this is a string handle.
    #[inline(always)]
    pub const fn is_string(&self) -> bool {
        self.is_boxed() && self.tag() == TAG_STR
    }

    /// True iff this is an object handle. (Does NOT count `null`, matching the
    /// tag — `null`'s `typeof` is `"object"` but it is a singleton, not a heap
    /// object.)
    #[inline(always)]
    pub const fn is_object(&self) -> bool {
        self.is_boxed() && self.tag() == TAG_OBJECT
    }

    /// True iff this is a function handle.
    #[inline(always)]
    pub const fn is_function(&self) -> bool {
        self.is_boxed() && self.tag() == TAG_FUNCTION
    }

    /// True iff this is the `undefined` singleton.
    #[inline(always)]
    pub const fn is_undefined(&self) -> bool {
        self.0 == Self::undefined().0
    }

    /// True iff this is the `null` singleton.
    #[inline(always)]
    pub const fn is_null(&self) -> bool {
        self.0 == Self::null().0
    }

    /// True iff this is `true` or `false`.
    #[inline(always)]
    pub const fn is_bool(&self) -> bool {
        self.0 == Self::bool(true).0 || self.0 == Self::bool(false).0
    }

    /// True iff this is the array-hole singleton.
    #[inline(always)]
    pub const fn is_hole(&self) -> bool {
        self.0 == Self::hole().0
    }

    /// True iff this is the internal empty singleton.
    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.0 == Self::empty().0
    }

    /// JS `ToBoolean` for the represented value, where it can be decided without
    /// touching the heap.
    ///
    /// - doubles: falsy iff `+0`, `-0`, or `NaN`; otherwise truthy.
    /// - int32: falsy iff `0`.
    /// - `false`/`null`/`undefined`/hole/empty: falsy.
    /// - `true`: truthy.
    /// - strings: a string handle is truthy iff non-empty, but that needs the
    ///   heap (the length lives in the HandleTable). We conservatively report
    ///   `true` here and document that the empty-string case must be resolved by
    ///   the caller with heap access. Objects/functions are always truthy.
    #[inline]
    pub fn is_truthy(&self) -> bool {
        if self.is_double() {
            let f = f64::from_bits(self.0);
            return f != 0.0 && !f.is_nan();
        }
        match self.tag() {
            TAG_INT32 => self.as_i32() != 0,
            TAG_SINGLETON => matches!(self.payload(), SINGLETON_TRUE),
            // String truthiness depends on length (heap) — non-empty is truthy.
            // Reported `true` here; empty-string falsiness is the caller's job.
            TAG_STR => true,
            // Any object or function is truthy in JS.
            TAG_OBJECT | TAG_FUNCTION => true,
            _ => false,
        }
    }

    // ---- accessors (debug-asserted on tag) ----

    /// Read the inline double. Debug-asserts the value is a double.
    #[inline(always)]
    pub fn as_f64(&self) -> f64 {
        debug_assert!(self.is_double(), "as_f64 on a boxed (non-double) value");
        f64::from_bits(self.0)
    }

    /// Read the tagged `int32`, sign-extended from the low 32 payload bits.
    /// Debug-asserts the tag is `int32`.
    #[inline(always)]
    pub fn as_i32(&self) -> i32 {
        debug_assert!(self.is_int32(), "as_i32 on a non-int32 value");
        // Low 32 bits hold the i32 as-stored; reinterpret to recover the sign.
        (self.0 as u32) as i32
    }

    /// Read the 48-bit handle slot. Debug-asserts the tag is a handle-carrying
    /// kind (string / object / function).
    #[inline(always)]
    pub fn as_handle(&self) -> u64 {
        debug_assert!(
            self.is_string() || self.is_object() || self.is_function(),
            "as_handle on a value that carries no handle"
        );
        self.payload()
    }

    // ---- typeof ----

    /// The JS `typeof` string for the represented value.
    ///
    /// JS quirk preserved: `typeof null === "object"` (a 30-year-old bug
    /// codified into the spec). Booleans are `"boolean"`, `undefined` is
    /// `"undefined"`, both number kinds are `"number"`. The internal hole/empty
    /// singletons report `"undefined"` (they are not user-observable, but if one
    /// leaks, `undefined` is the least-surprising answer).
    pub fn typeof_str(&self) -> &'static str {
        if self.is_double() {
            return "number";
        }
        match self.tag() {
            TAG_INT32 => "number",
            TAG_STR => "string",
            TAG_OBJECT => "object",
            TAG_FUNCTION => "function",
            TAG_SINGLETON => match self.payload() {
                SINGLETON_NULL => "object", // JS quirk: typeof null === "object"
                SINGLETON_FALSE | SINGLETON_TRUE => "boolean",
                // undefined, hole, empty
                _ => "undefined",
            },
            // Reserved tags (0/6/7) — should not occur; least-surprising default.
            _ => "undefined",
        }
    }
}

impl core::fmt::Debug for PolyValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_double() {
            write!(f, "PolyValue::Double({})", self.as_f64())
        } else {
            match self.tag() {
                TAG_INT32 => write!(f, "PolyValue::Int32({})", self.as_i32()),
                TAG_STR => write!(f, "PolyValue::Str(#{})", self.payload()),
                TAG_OBJECT => write!(f, "PolyValue::Object(#{})", self.payload()),
                TAG_FUNCTION => write!(f, "PolyValue::Function(#{})", self.payload()),
                TAG_SINGLETON => {
                    let name = match self.payload() {
                        SINGLETON_UNDEFINED => "undefined",
                        SINGLETON_NULL => "null",
                        SINGLETON_FALSE => "false",
                        SINGLETON_TRUE => "true",
                        SINGLETON_HOLE => "hole",
                        SINGLETON_EMPTY => "empty",
                        other => return write!(f, "PolyValue::Singleton(?{other})"),
                    };
                    write!(f, "PolyValue::{name}")
                }
                other => write!(f, "PolyValue::Reserved(tag={other}, raw={:#018x})", self.0),
            }
        }
    }
}

// ===========================================================================
// Cranelift emit helpers
//
// These prove that box/unbox is pure straight-line IR (band/bor/icmp/bitcast/
// ishl/sshr) — no extern call — that Cranelift's egraph can fold. Each takes a
// live `FunctionBuilder` and operates on i64-typed SSA `Value`s (the ABI slot
// for a PolyValue) except where noted.
// ===========================================================================

/// Emit `(v & BOX_BASE) == BOX_BASE`, producing an `i8` boolean (1 = boxed).
///
/// `v` must be the i64 raw word.
pub fn emit_is_boxed(builder: &mut FunctionBuilder, v: Value) -> Value {
    let base = builder.ins().iconst(types::I64, BOX_BASE as i64);
    let masked = builder.ins().band(v, base);
    builder.ins().icmp(IntCC::Equal, masked, base)
}

/// Emit the negation of [`emit_is_boxed`]: `(v & BOX_BASE) != BOX_BASE`,
/// i.e. "is an inline double". Produces an `i8` boolean (1 = double).
pub fn emit_is_double(builder: &mut FunctionBuilder, v: Value) -> Value {
    let base = builder.ins().iconst(types::I64, BOX_BASE as i64);
    let masked = builder.ins().band(v, base);
    builder.ins().icmp(IntCC::NotEqual, masked, base)
}

/// Box an `i32` SSA value (Cranelift type `I32`) into a tagged-`int32`
/// PolyValue word (i64).
///
/// `encode(TAG_INT32, i as u32) = BOX_BASE | (TAG_INT32<<48) | (i as u32)`.
/// We zero-extend the i32 to i64 (so the high payload bits are 0) then OR in the
/// constant header.
pub fn emit_box_int32(builder: &mut FunctionBuilder, i32_val: Value) -> Value {
    // Zero-extend the 32-bit value into the low 32 bits of an i64.
    let widened = builder.ins().uextend(types::I64, i32_val);
    let header = encode(TAG_INT32, 0) as i64; // BOX_BASE | (TAG_INT32<<48)
    let header_v = builder.ins().iconst(types::I64, header);
    builder.ins().bor(widened, header_v)
}

/// Unbox a tagged-`int32` PolyValue word (i64) back to an i64 holding the
/// sign-extended `i32` value (matching [`PolyValue::as_i32`] cast to `i64`).
///
/// Done branch-free with shifts: mask is implicit because the payload's low
/// 32 bits already hold the int; `ishl 32` pushes the sign bit (bit 31) to bit
/// 63, then `sshr 32` arithmetic-shifts it back, sign-extending. This drops the
/// tag/header bits (they live above bit 48 and are shifted out) for free.
pub fn emit_unbox_int32(builder: &mut FunctionBuilder, v: Value) -> Value {
    let shifted_up = builder.ins().ishl_imm(v, 32);
    builder.ins().sshr_imm(shifted_up, 32)
}

/// Box an `f64` SSA value into its inline-double PolyValue word: a plain
/// `bitcast` f64→i64.
///
/// NOTE: the pure-Rust model canonicalizes NaN in [`PolyValue::from_f64`]. That
/// requires a compare+select (control-flow-free, but two extra ops) and is left
/// out of this straight-line helper on purpose — the box-double fast path is for
/// values the front-end already proved are not NaN, or where the boxed word is
/// immediately unboxed again. A NaN-canonicalizing variant would be:
/// `select(is_nan(f), iconst(CANONICAL_NAN), bitcast(f))`. Callers that may box
/// an arbitrary NaN-bearing double must insert that select (TODO when the lower
/// pass needs it).
pub fn emit_box_double(builder: &mut FunctionBuilder, f64_val: Value) -> Value {
    builder
        .ins()
        .bitcast(types::I64, MemFlags::new(), f64_val)
}

/// Unbox an inline-double PolyValue word (i64) back to an `f64`: a plain
/// `bitcast` i64→f64.
pub fn emit_unbox_double(builder: &mut FunctionBuilder, v: Value) -> Value {
    builder.ins().bitcast(types::F64, MemFlags::new(), v)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers --

    /// Doubles representative of every interesting class — none of which may be
    /// classified as boxed (NaN is handled separately because of canonicalization).
    fn representative_doubles() -> Vec<f64> {
        vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            1.5,
            -2.25,
            3.141592653589793,
            f64::MIN,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::MIN_POSITIVE / 2.0, // subnormal
            f64::INFINITY,
            f64::NEG_INFINITY,
            1e308,
            -1e308,
            123456789.0,
        ]
    }

    // ----------------------------------------------------------------
    // 1. Pure-model unit tests
    // ----------------------------------------------------------------

    #[test]
    fn doubles_roundtrip_and_classify() {
        for &d in &representative_doubles() {
            let v = PolyValue::from_f64(d);
            assert!(v.is_double(), "{d} should classify as a double");
            assert!(!v.is_boxed(), "{d} must not be in boxed space");
            assert!(!v.is_int32());
            assert!(!v.is_string());
            assert!(!v.is_object());
            assert!(!v.is_function());
            // exact round-trip
            let back = v.as_f64();
            assert_eq!(back.to_bits(), d.to_bits(), "{d} did not round-trip exactly");
            assert_eq!(v.typeof_str(), "number");
        }
    }

    #[test]
    fn signed_zero_is_preserved() {
        let pos = PolyValue::from_f64(0.0);
        let neg = PolyValue::from_f64(-0.0);
        assert!(pos.is_double() && neg.is_double());
        // +0.0 and -0.0 are bit-distinct and the sign must survive.
        assert_eq!(pos.as_f64().to_bits(), 0u64);
        assert_eq!(neg.as_f64().to_bits(), 0x8000_0000_0000_0000u64);
        assert!(pos.as_f64() == 0.0 && neg.as_f64() == 0.0);
        assert!(pos.as_f64().is_sign_positive());
        assert!(neg.as_f64().is_sign_negative());
        // is_truthy: both zeroes are falsy.
        assert!(!pos.is_truthy());
        assert!(!neg.is_truthy());
    }

    #[test]
    fn neg_infinity_is_a_double_not_boxed() {
        // -Infinity = 0xFFF0_0000_0000_0000. Bit 51 is 0, so it is NOT boxed.
        let v = PolyValue::from_f64(f64::NEG_INFINITY);
        assert_eq!(v.raw(), 0xFFF0_0000_0000_0000);
        assert!(v.is_double(), "-Infinity must classify as a double");
        assert!(!v.is_boxed(), "-Infinity must not be in boxed space");
        assert_eq!(v.as_f64(), f64::NEG_INFINITY);
        assert!(v.is_truthy()); // -Infinity is truthy in JS
        assert_eq!(v.typeof_str(), "number");

        // +Infinity for symmetry.
        let p = PolyValue::from_f64(f64::INFINITY);
        assert!(p.is_double() && !p.is_boxed());
        assert_eq!(p.as_f64(), f64::INFINITY);
    }

    #[test]
    fn nan_is_canonicalized_and_classifies_as_double() {
        let v = PolyValue::from_f64(f64::NAN);
        // Stored as the positive canonical qNaN, NOT in boxed space.
        assert_eq!(v.raw(), CANONICAL_NAN);
        assert!(v.is_double(), "canonical NaN must classify as a double");
        assert!(!v.is_boxed(), "canonical NaN must NOT be in boxed space");
        assert!(v.as_f64().is_nan(), "must round-trip to a NaN");
        assert!(!v.is_truthy(), "NaN is falsy");
        assert_eq!(v.typeof_str(), "number");

        // A *negative* qNaN input is also canonicalized to the positive one, so
        // it never lands in the boxed space (this is the soundness guarantee).
        let neg_qnan = f64::from_bits(0xFFF8_0000_0000_0001);
        assert!(neg_qnan.is_nan());
        let v2 = PolyValue::from_f64(neg_qnan);
        assert_eq!(v2.raw(), CANONICAL_NAN);
        assert!(v2.is_double() && !v2.is_boxed());
    }

    #[test]
    fn int32_roundtrip() {
        for &i in &[0i32, 1, -1, 42, -42, i32::MIN, i32::MAX, 0x7FFF_FFFF, -0x8000_0000] {
            let v = PolyValue::from_i32(i);
            assert!(v.is_int32(), "{i} should be int32");
            assert!(v.is_boxed());
            assert!(!v.is_double(), "{i} must not classify as a double");
            assert_eq!(v.as_i32(), i, "{i} did not round-trip");
            assert_eq!(v.tag(), TAG_INT32);
            assert_eq!(v.typeof_str(), "number");
        }
        // truthiness: 0 falsy, everything else truthy.
        assert!(!PolyValue::from_i32(0).is_truthy());
        assert!(PolyValue::from_i32(1).is_truthy());
        assert!(PolyValue::from_i32(-1).is_truthy());
    }

    #[test]
    fn handles_roundtrip() {
        let max48: u64 = PAYLOAD_MASK; // 0xFFFF_FFFF_FFFF
        for &slot in &[0u64, 1, 0xDEAD_BEEF, 0xFFFF_FFFF_FFFF, max48] {
            // string
            let s = PolyValue::from_str_handle(slot);
            assert!(s.is_string());
            assert!(!s.is_object() && !s.is_function() && !s.is_double());
            assert_eq!(s.as_handle(), slot);
            assert_eq!(s.typeof_str(), "string");
            assert_eq!(s.tag(), TAG_STR);

            // object
            let o = PolyValue::from_object_handle(slot);
            assert!(o.is_object());
            assert!(!o.is_string() && !o.is_function() && !o.is_double());
            assert_eq!(o.as_handle(), slot);
            assert_eq!(o.typeof_str(), "object");
            assert_eq!(o.tag(), TAG_OBJECT);

            // function
            let fnh = PolyValue::from_function_handle(slot);
            assert!(fnh.is_function());
            assert!(!fnh.is_string() && !fnh.is_object() && !fnh.is_double());
            assert_eq!(fnh.as_handle(), slot);
            assert_eq!(fnh.typeof_str(), "function");
            assert_eq!(fnh.tag(), TAG_FUNCTION);

            // all three with the same slot must be DISTINCT words (tag differs).
            assert_ne!(s.raw(), o.raw());
            assert_ne!(o.raw(), fnh.raw());
            assert_ne!(s.raw(), fnh.raw());

            // objects/functions/strings are truthy (string-emptiness aside).
            assert!(o.is_truthy());
            assert!(fnh.is_truthy());
            assert!(s.is_truthy());
        }
    }

    #[test]
    fn singletons_distinct_and_correct() {
        let undef = PolyValue::undefined();
        let null = PolyValue::null();
        let t = PolyValue::bool(true);
        let f = PolyValue::bool(false);
        let hole = PolyValue::hole();
        let empty = PolyValue::empty();

        // predicates
        assert!(undef.is_undefined());
        assert!(null.is_null());
        assert!(t.is_bool() && f.is_bool());
        assert!(hole.is_hole());
        assert!(empty.is_empty());

        // none of these is a double / number / heap kind
        for s in [undef, null, t, f, hole, empty] {
            assert!(!s.is_double(), "{s:?} must not be a double");
            assert!(s.is_boxed());
            assert!(!s.is_int32() && !s.is_string() && !s.is_object() && !s.is_function());
            assert_eq!(s.tag(), TAG_SINGLETON);
        }

        // typeof
        assert_eq!(undef.typeof_str(), "undefined");
        assert_eq!(null.typeof_str(), "object"); // JS quirk
        assert_eq!(t.typeof_str(), "boolean");
        assert_eq!(f.typeof_str(), "boolean");

        // truthiness
        assert!(!undef.is_truthy());
        assert!(!null.is_truthy());
        assert!(t.is_truthy());
        assert!(!f.is_truthy());
        assert!(!hole.is_truthy());
        assert!(!empty.is_truthy());

        // all six singletons are pairwise bit-distinct
        let all = [undef, null, t, f, hole, empty];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(
                    all[i].raw(),
                    all[j].raw(),
                    "singletons {i} and {j} share raw bits"
                );
            }
        }
    }

    #[test]
    fn disjointness_no_double_is_boxed() {
        // Every non-double constructor output must NOT be a double.
        let mut non_doubles: Vec<PolyValue> = vec![
            PolyValue::undefined(),
            PolyValue::null(),
            PolyValue::bool(true),
            PolyValue::bool(false),
            PolyValue::hole(),
            PolyValue::empty(),
        ];
        for &i in &[0i32, -1, i32::MIN, i32::MAX, 12345] {
            non_doubles.push(PolyValue::from_i32(i));
        }
        for &slot in &[0u64, 1, PAYLOAD_MASK] {
            non_doubles.push(PolyValue::from_str_handle(slot));
            non_doubles.push(PolyValue::from_object_handle(slot));
            non_doubles.push(PolyValue::from_function_handle(slot));
        }
        for v in &non_doubles {
            assert!(!v.is_double(), "{v:?} must not be a double");
            assert!(v.is_boxed(), "{v:?} must be boxed");
        }

        // And a sweep of doubles must all be is_double and NOT mis-tagged.
        let mut doubles = representative_doubles();
        doubles.push(f64::NAN);
        for &d in &doubles {
            let v = PolyValue::from_f64(d);
            assert!(v.is_double(), "{d} must be a double");
            assert!(!v.is_int32());
            assert!(!v.is_string());
            assert!(!v.is_object());
            assert!(!v.is_function());
            assert!(!v.is_undefined() && !v.is_null() && !v.is_bool());
        }
    }

    #[test]
    fn raw_roundtrips_through_from_raw() {
        let samples = [
            PolyValue::from_f64(1.5),
            PolyValue::from_i32(-7),
            PolyValue::from_str_handle(99),
            PolyValue::from_object_handle(0xABCD),
            PolyValue::from_function_handle(0xFFFF_FFFF_FFFF),
            PolyValue::undefined(),
            PolyValue::null(),
            PolyValue::bool(true),
        ];
        for v in samples {
            assert_eq!(PolyValue::from_raw(v.raw()), v);
        }
    }

    #[test]
    fn encode_header_constants_are_what_the_docs_claim() {
        // BOX_BASE has the top 13 bits set and nothing else.
        assert_eq!(BOX_BASE, 0xFFF8_0000_0000_0000);
        assert_eq!(BOX_BASE >> 51, 0x1FFF); // 13 ones
        assert_eq!(BOX_BASE & !(0x1FFFu64 << 51), 0);
        // int32 header = BOX_BASE | (1<<48)
        assert_eq!(encode(TAG_INT32, 0), 0xFFF9_0000_0000_0000);
        // singleton header = BOX_BASE | (2<<48)
        assert_eq!(encode(TAG_SINGLETON, 0), 0xFFFA_0000_0000_0000);
        // CANONICAL_NAN is a double (positive qNaN), not boxed.
        assert_eq!(CANONICAL_NAN, 0x7FF8_0000_0000_0000);
        assert!(PolyValue::from_raw(CANONICAL_NAN).is_double());
    }

    // ----------------------------------------------------------------
    // 2. Cranelift JIT roundtrip — proves the emitted IR matches the model.
    // ----------------------------------------------------------------

    #[test]
    fn jit_unbox_int32_matches_model() {
        use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature};
        use cranelift_codegen::settings::{self, Configurable};
        use cranelift_codegen::ir::types;
        use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
        use cranelift_jit::{JITBuilder, JITModule};
        use cranelift_module::{Linkage, Module};

        // Build an ISA for the host.
        let mut flags = settings::builder();
        flags.set("opt_level", "speed").unwrap();
        let isa = cranelift_native::builder()
            .expect("host isa builder")
            .finish(settings::Flags::new(flags))
            .expect("finish isa");

        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let mut module = JITModule::new(builder);

        // Signature: extern "C" fn(i64) -> i64. Takes a raw PolyValue word
        // (assumed tagged int32), returns the sign-extended i32 (as i64).
        let mut sig = Signature::new(module.isa().default_call_conv());
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("test_unbox_int32", Linkage::Local, &sig)
            .expect("declare");

        let mut ctx = module.make_context();
        ctx.func.signature = sig;

        {
            let mut fb_ctx = FunctionBuilderContext::new();
            let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
            let entry = fb.create_block();
            fb.append_block_params_for_function_params(entry);
            fb.switch_to_block(entry);
            fb.seal_block(entry);

            let arg = fb.block_params(entry)[0];
            let result = emit_unbox_int32(&mut fb, arg);
            fb.ins().return_(&[result]);
            fb.finalize();
        }

        module.define_function(func_id, &mut ctx).expect("define");
        module.clear_context(&mut ctx);
        module.finalize_definitions().expect("finalize");

        let code = module.get_finalized_function(func_id);
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(code) };

        for &i in &[0i32, 1, -1, 42, -42, i32::MIN, i32::MAX, 123456, -987654] {
            let boxed = PolyValue::from_i32(i);
            let model = PolyValue::from_raw(boxed.raw()).as_i32() as i64;
            let jitted = f(boxed.raw() as i64);
            assert_eq!(jitted, model, "JIT unbox_int32 mismatch for {i}");
            assert_eq!(jitted, i as i64, "JIT unbox_int32 wrong value for {i}");
        }
    }

    #[test]
    fn jit_is_boxed_matches_model() {
        use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature};
        use cranelift_codegen::settings::{self, Configurable};
        use cranelift_codegen::ir::types;
        use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
        use cranelift_jit::{JITBuilder, JITModule};
        use cranelift_module::{Linkage, Module};

        let mut flags = settings::builder();
        flags.set("opt_level", "speed").unwrap();
        let isa = cranelift_native::builder()
            .expect("host isa builder")
            .finish(settings::Flags::new(flags))
            .expect("finish isa");

        let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        let mut module = JITModule::new(builder);

        // fn(i64) -> i64 returning 1 if boxed else 0.
        let mut sig = Signature::new(module.isa().default_call_conv());
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module
            .declare_function("test_is_boxed", Linkage::Local, &sig)
            .expect("declare");

        let mut ctx = module.make_context();
        ctx.func.signature = sig;

        {
            let mut fb_ctx = FunctionBuilderContext::new();
            let mut fb = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
            let entry = fb.create_block();
            fb.append_block_params_for_function_params(entry);
            fb.switch_to_block(entry);
            fb.seal_block(entry);

            let arg = fb.block_params(entry)[0];
            let is_boxed_i8 = emit_is_boxed(&mut fb, arg);
            // widen the i8 bool to i64 for the return.
            let widened = fb.ins().uextend(types::I64, is_boxed_i8);
            fb.ins().return_(&[widened]);
            fb.finalize();
        }

        module.define_function(func_id, &mut ctx).expect("define");
        module.clear_context(&mut ctx);
        module.finalize_definitions().expect("finalize");

        let code = module.get_finalized_function(func_id);
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(code) };

        let cases: Vec<PolyValue> = vec![
            PolyValue::from_f64(0.0),
            PolyValue::from_f64(-0.0),
            PolyValue::from_f64(1.5),
            PolyValue::from_f64(f64::INFINITY),
            PolyValue::from_f64(f64::NEG_INFINITY),
            PolyValue::from_f64(f64::NAN),
            PolyValue::from_i32(7),
            PolyValue::from_i32(-7),
            PolyValue::from_str_handle(42),
            PolyValue::from_object_handle(0xABCD),
            PolyValue::from_function_handle(1),
            PolyValue::undefined(),
            PolyValue::null(),
            PolyValue::bool(true),
            PolyValue::bool(false),
            PolyValue::hole(),
            PolyValue::empty(),
        ];
        for v in cases {
            let model = if v.is_boxed() { 1i64 } else { 0 };
            let jitted = f(v.raw() as i64);
            assert_eq!(jitted, model, "JIT is_boxed mismatch for {v:?}");
        }
    }
}
