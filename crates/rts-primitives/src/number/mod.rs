//! `Number` — the REAL primordial `Number` for the new engine, authored as a
//! FULLY SELF-CONTAINED pure-Rust value-class with `#[rtse::class("Number",
//! value)]`, following the SAME pattern `String`/`Boolean` proved
//! (`rts-primitives/src/string/value_class.rs`, `rts-primitives/src/boolean.rs`).
//! Replaces both the hand-written `Member` literals this module used to carry
//! for the ctor + instance methods (the `__RTS_FN_GL_NUMBER_*` externs) AND the
//! ambient `.ts class Number` (`number.ts`, now deleted).
//!
//! ## Dual `this`: autoboxed primitive vs wrapper object
//! A method reaches EITHER receiver (the engine is shape-based, not prototypes):
//!   - `(5).toFixed(2)` — the PRIMITIVE is autoboxed as the receiver word (an
//!     inline `f64` or a boxed `INT32` PolyValue). [`num_val`] reads it directly
//!     via `poly_number`.
//!   - `new Number(5).toFixed(2)` — the receiver is the WRAPPER object (an
//!     `Entry::Rtse` classed "Number"); [`num_val`] reads its `prim` slot.
//! Every instance method takes the receiver as its FIRST typed `Poly` param (the
//! value-class ABI): the raw NaN-boxed word, unboxed in-body by [`num_val`].
//!
//! ## `Number(x)` — the call-WITHOUT-`new` form stays in codegen (NOT converted)
//! Unlike `Boolean`, `Number` does NOT declare `#[rtse::functioncall]`. JS gives
//! `Number()` (zero args) and `Number(undefined)` (one explicit `undefined` arg)
//! DIFFERENT results — `0` vs `NaN` — a distinction only visible at the AST
//! (argument COUNT), not at the value level (both arrive as the same `undefined`
//! word once lowered). The generic functioncall protocol pads a missing trailing
//! arg with the `undefined` word, which would collapse that distinction. So the
//! call form stays on `front/run/globals.rs`'s `"Number"` arm, which still
//! delegates to the engine's own ToNumber (`__rtsadp_g_number`), never
//! reimplementing it — see that file for the zero-arg special case.
//!
//! ## Statics/constants stay data-driven, formatting stays in Rust
//! `Number.isNaN`/`isFinite`/`isInteger`/`isSafeInteger` and the 8 numeric
//! constants (`MAX_SAFE_INTEGER`, …) are ALREADY resolved by a dedicated codegen
//! fast path (`front/run/mathobj.rs`) that calls the `__RTS_FN_GL_NUMBER_IS_*`
//! externs directly by symbol for a proven-number arg (falling to the no-coerce
//! `__rtsadp_num_is_*` trampolines for a Tagged arg — the strict, non-coercing
//! semantics `Number.isFinite("5") === false` requires). Those externs (in
//! `statics.rs`) are load-bearing, not legacy. The `Member` entries
//! `statics::register_number_statics` builds are the Registry-visible mirror of
//! that same surface (reflection, `.d.ts` generation, and any call site the
//! codegen fast path does not special-case) — composed onto the SAME "Number"
//! Registry class entry as this module's macro-generated ctor/methods via
//! `Registry::insert_class`'s merge-on-second-call (a `#[rtse::class]` `impl`
//! block cannot carry a `const`, so the 8 `Number.*` constants cannot be
//! expressed as members of the SAME macro invocation).

mod format;
mod statics;

pub use statics::{number_const_value, register_number_statics};

use rts_engine::abi::ty::Poly;
use rts_engine::heap::handles::{rtse_class_of, with_rtse};
use rts_engine::heap::poly::{POLY_UNDEFINED, poly_handle_normalize, poly_number};

use format::{to_exponential_str, to_fixed_str, to_precision_str, to_string_radix};

// JS `ToNumber` on a raw PolyValue word — defined in `rts-adapters` (above this
// crate), reached via a forward `extern "C"` decl (layering-safe: rts-primitives
// is an rlib, the symbol resolves at the final link). Same pattern
// `boolean.rs`/`string/value_class.rs` use for their own coercion trampolines.
unsafe extern "C" {
    fn __rtsadp_g_number(word: u64) -> u64;
}

/// Backs the wrapper object form `new Number(x)` (typeof `"object"`): `prim` is
/// the wrapped primitive number. A PRIMITIVE number is NEVER a `NumberWrapper` —
/// it stays an inline `f64`/`INT32` PolyValue and reaches the methods through the
/// autobox path.
#[rtse::class("Number", value)]
#[derive(Clone)]
pub struct NumberWrapper {
    prim: f64,
}

/// The primitive number a receiver word denotes, unifying both receiver forms: a
/// wrapper reads its `prim` slot; an autoboxed primitive reads its own numeric
/// word directly (`poly_number`). A non-number wrapper-less word (should not
/// happen — the engine only dispatches here for a proven Number receiver) reads
/// as `NaN`, never a panic.
fn num_val(recv: u64) -> f64 {
    let h = poly_handle_normalize(recv).unwrap_or(recv);
    if rtse_class_of(h) == Some("Number") {
        return with_rtse::<NumberWrapper, _>(h, |w| w.map(|w| w.prim).unwrap_or(f64::NAN));
    }
    poly_number(recv).unwrap_or(f64::NAN)
}

#[rtse::class("Number", value)]
impl NumberWrapper {
    /// `new Number(value?)` — the wrapper OBJECT. `new Number()` (or an explicit
    /// `undefined`, indistinguishable to the engine at this level — see the module
    /// doc's note on the `Number()`-vs-`Number(undefined)` distinction) reads as
    /// `0`, matching the OLD dedicated zero-arg ctor's behavior; else `ToNumber`
    /// via the engine's own coercion.
    #[rtse::ctor(optional = 1)]
    fn new(value: Poly) -> Self {
        if value == POLY_UNDEFINED {
            return NumberWrapper { prim: 0.0 };
        }
        let coerced = unsafe { __rtsadp_g_number(value) };
        NumberWrapper { prim: poly_number(coerced).unwrap_or(f64::NAN) }
    }

    /// `n.valueOf()` — the primitive number itself.
    #[rtse::method]
    fn value_of(recv: Poly) -> f64 {
        num_val(recv)
    }

    /// `n.toString(radix?)` — base-10 string by default; `radix` (2..36) selects
    /// another base. `Option<f64>` (absent → `None`) expresses the default
    /// PRECISELY (an omitted radix is base 10, not the `-1`/auto sentinel
    /// `toPrecision`/`toExponential` use below).
    #[rtse::method]
    fn to_string(recv: Poly, radix: Option<f64>) -> String {
        to_string_radix(num_val(recv), radix.unwrap_or(10.0) as i64)
    }

    /// `n.toLocaleString()` — no locale data in the runtime, so it defers to
    /// base-10 `toString` (matches the runtime's existing behavior).
    #[rtse::method]
    fn to_locale_string(recv: Poly) -> String {
        to_string_radix(num_val(recv), 10)
    }

    /// `n.toFixed(digits?)` — fixed-point notation, default 0 fraction digits.
    #[rtse::method]
    fn to_fixed(recv: Poly, digits: Option<f64>) -> String {
        to_fixed_str(num_val(recv), digits.unwrap_or(0.0) as i64)
    }

    /// `n.toPrecision(precision?)` — `precision` significant digits; an omitted
    /// (or non-positive) precision renders as plain `toString` (the `-1` auto
    /// sentinel the formatter already treats as "not given").
    #[rtse::method]
    fn to_precision(recv: Poly, precision: Option<f64>) -> String {
        to_precision_str(num_val(recv), precision.unwrap_or(-1.0) as i64)
    }

    /// `n.toExponential(digits?)` — exponential notation; an omitted (negative)
    /// `digits` picks the shortest faithful mantissa.
    #[rtse::method]
    fn to_exponential(recv: Poly, digits: Option<f64>) -> String {
        to_exponential_str(num_val(recv), digits.unwrap_or(-1.0) as i64)
    }
}
