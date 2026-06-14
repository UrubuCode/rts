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
//! ## Module layout
//!
//! - `mod.rs` (this file): the [`PolyValue`] type, bit-layout constants, tag /
//!   singleton constants, constructors, predicates, accessors, `typeof_str`,
//!   `Debug`. Re-exports the Cranelift emit helpers from [`emit`].
//! - [`emit`]: the Cranelift `emit_*` helpers (pure straight-line IR for
//!   box/unbox/classify).
//! - `tests` (`#[cfg(test)]`): the pure-model + JIT-roundtrip unit tests.
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

mod emit;
// Bit-layout constants + `encode`, split out to keep this file under 500 lines.
// Re-exported below so call sites (`crate::value::BOX_BASE`, `super::encode`, …)
// resolve unchanged.
mod layout;
#[cfg(test)]
mod tests;

pub use layout::*;

// The ABI adapter: the indirection table + host helpers + `__rtsadp_*` table
// trampolines that bridge PolyValue to the REAL runtime string pool.
pub mod abi_adapter;
// The real-symbol signature descriptors (fixes the StrPtr=2-slots bug).
pub mod abi_sig;
// The generic JS operators (`__rtsadp_add`/`_strict_eq`/`_typeof`/…), backed by
// the real string pool — the ONE tag-dispatched path.
pub mod genops;
// The generic JS arithmetic / comparison / unary / bitwise operators (P4.8):
// `__rtsadp_sub`/`_mul`/`_div`/`_mod`/`_pow`/`_lt`/…/`_band`/… on Tagged operands.
pub mod genops_arith;
// Codegen-owned PolyValue-aware Array instance-method trampolines (P4.5),
// reading the REAL Entry::Vec via the runtime VEC symbols.
pub mod arrayops;
// Codegen-owned first-class FUNCTION-value trampolines (P4.6): reify a user fn
// (its thunk address) into a TAG_FUNCTION PolyValue + invoke it indirectly via
// the fixed uniform 5-slot ABI.
pub mod funcops;
// Codegen-owned Array CALLBACK trampolines (P4.7): map/filter/forEach/find/
// findIndex/some/every/reduce, invoking the callback function VALUE per element
// via funcops::__rtsadp_fn_invoke.
pub mod arraycb;
// Codegen-owned GLOBAL constant/function + Array/String STATIC trampolines (P5.2):
// Number/String/Boolean/parseInt/parseFloat/isNaN/isFinite + Array.isArray/of/from
// + Array(n) + String.fromCharCode/fromCodePoint + str.split (PolyValue-native).
pub mod globalops;
// Codegen-owned Map/Set instance trampolines (P5.3): PolyValue-native new/get/set/
// has/delete/clear/size + add, over the REAL collections MAP_*/SET_* symbols.
pub mod mapset;
// Codegen-owned RegExp + string-regex-method trampolines (P5.12): compile a
// `/pat/flags` literal / `new RegExp(..)`, `.test`/`.source`/`.flags`/`.global`,
// and `s.match`/`.replace`/`.replaceAll`/`.split`/`.search`, over the REAL
// `__RTS_FN_NS_REGEX_*` symbols (array results built codegen-side).
pub mod regexops;
// Codegen-owned DYNAMIC property-access trampolines (P5.5): `__rtsadp_obj_get`/
// `_set`/`_has` for `obj.key`/`obj[k]` whose shape is known only at RUNTIME,
// reading the slot-0 global shape-id + the global shape registry.
pub mod objops;
// Codegen-owned Error-family + Boolean/Number/String wrapper constructors, Error
// instance props, and `instanceof` runtime tags (P5.3).
pub mod wrappers;
// Codegen-owned DYNAMIC (runtime) method-dispatch trampolines (P5.9):
// `__rtsadp_dyn_*(recv_word, args)` branch on the receiver's PolyValue tag at
// runtime and delegate to the per-class op — for `recv.method(args)` whose
// receiver class is NOT statically proven (a Tagged param/return/local).
pub mod dyndispatch;
// The Cranelift IR that marshals at the real-symbol call boundaries
// (box/unbox + StrPtr ptr+len).
pub mod emit_marshal;
// console.log PRETTY-PRINT of whole ARRAYS (Bun/Node util.inspect format, P3.5):
// `__rtsadp_inspect(value_word, top_level)` → a string PolyValue word.
pub mod inspect;
// Codegen-owned ITERATION-source trampolines (P5.10): materialize a string's code
// points / an object's keys into a fresh array, so for-of/for-in share ONE walk.
pub mod iterops;

// Re-export the Cranelift emit helpers so existing call sites
// (`crate::value::emit_box_int32`, …) keep resolving unchanged.
pub use emit::{
    emit_box_double, emit_box_int32, emit_is_boxed, emit_is_double, emit_tagged_number_to_f64,
    emit_unbox_double, emit_unbox_int32,
};

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
    pub(crate) const fn payload(&self) -> u64 {
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

    /// Read this value as a JS boolean, assuming it IS a boolean singleton.
    /// Debug-asserts `is_bool()`.
    #[inline(always)]
    pub fn as_bool(&self) -> bool {
        debug_assert!(self.is_bool(), "as_bool on a non-boolean value");
        self.0 == Self::bool(true).0
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

    /// Read the represented number as `f64`, for either number kind (int32 or
    /// double). Debug-asserts the value is a number.
    #[inline(always)]
    pub fn number_as_f64(&self) -> f64 {
        debug_assert!(self.is_double() || self.is_int32(), "number_as_f64 on a non-number");
        if self.is_int32() {
            self.as_i32() as f64
        } else {
            self.as_f64()
        }
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
