//! `Boolean` — the REAL primordial `Boolean` for the new engine, authored as a
//! FULLY SELF-CONTAINED pure-Rust value-class with `#[rtse::class("Boolean",
//! value)]`, following the SAME pattern `String` proved
//! (`rts-primitives/src/string/value_class.rs`). Replaces both the hand-written
//! `Member` literals this file used to carry (the `__RTS_FN_GL_BOOLEAN_*`
//! externs, unreachable from any live codegen path — shadowed by the ambient
//! `.ts class Boolean` this migration also removes) AND `boolean.ts` itself.
//!
//! ## Dual `this`: autoboxed primitive vs wrapper object
//! A method reaches EITHER receiver (the engine is shape-based, not prototypes):
//!   - `true.toString()` — the PRIMITIVE is autoboxed as the receiver word (a
//!     `TAG_SINGLETON` PolyValue, `POLY_TRUE`/`POLY_FALSE`). [`bool_val`] reads
//!     it directly.
//!   - `new Boolean(false).valueOf()` — the receiver is the WRAPPER object (an
//!     `Entry::Rtse` classed "Boolean"); [`bool_val`] reads its `prim` slot.
//! Every instance method takes the receiver as its FIRST typed `Poly` param (the
//! value-class ABI): the raw NaN-boxed word, unboxed in-body by [`bool_val`].
//!
//! ## `Boolean(x)` — the call-WITHOUT-`new` form
//! `#[rtse::functioncall]` declares the coercion member the engine resolves via
//! `registry::class_functioncall("Boolean")` (DATA, no class name in codegen
//! control flow — see `crates/rts-codegen-new/src/front/run/globals.rs`). It
//! delegates to the engine's own `ToBoolean` (`__rtsadp_g_boolean`), never
//! re-implementing JS truthiness here.

use rts_engine::heap::poly::{POLY_TRUE, poly_handle_normalize};
use rts_engine::heap::handles::{rtse_class_of, with_rtse};

use rts_engine::abi::ty::Poly;

// JS `ToBoolean` on a raw PolyValue word — defined in `rts-adapters` (above this
// crate), reached via a forward `extern "C"` decl (layering-safe: rts-primitives
// is an rlib, the symbol resolves at the final link). Same pattern
// `string/value_class.rs` uses for `__rtsadp_to_string`.
unsafe extern "C" {
    fn __rtsadp_g_boolean(word: u64) -> u64;
}

/// Backs the wrapper object form `new Boolean(x)` (typeof `"object"`): `prim` is
/// the wrapped primitive boolean. A PRIMITIVE boolean is NEVER a
/// `BooleanWrapper` — it stays a `TAG_SINGLETON` PolyValue and reaches the
/// methods through the autobox path.
#[rtse::class("Boolean", value)]
#[derive(Clone)]
pub struct BooleanWrapper {
    prim: bool,
}

/// The primitive boolean a receiver word denotes, unifying both receiver forms:
/// a wrapper reads its `prim` slot; an autoboxed primitive singleton reads its
/// own tag directly (`recv == POLY_TRUE`).
fn bool_val(recv: u64) -> bool {
    let h = poly_handle_normalize(recv).unwrap_or(recv);
    if rtse_class_of(h) == Some("Boolean") {
        return with_rtse::<BooleanWrapper, _>(h, |w| w.map(|w| w.prim).unwrap_or(false));
    }
    recv == POLY_TRUE
}

#[rtse::class("Boolean", value)]
impl BooleanWrapper {
    /// `new Boolean(value?)` — the wrapper OBJECT. ToBoolean via the engine's
    /// own coercion (an omitted arg reads as `undefined`, which `ToBoolean`
    /// maps to `false`, matching `new Boolean()`).
    #[rtse::ctor(optional = 1)]
    fn new(value: Poly) -> Self {
        let word = unsafe { __rtsadp_g_boolean(value) };
        BooleanWrapper { prim: word == POLY_TRUE }
    }

    /// `bool.toString()` — `"true"` / `"false"`.
    #[rtse::method]
    fn to_string(recv: Poly) -> String {
        if bool_val(recv) { "true".to_string() } else { "false".to_string() }
    }

    /// `bool.valueOf()` — the primitive boolean itself.
    #[rtse::method]
    fn value_of(recv: Poly) -> bool {
        bool_val(recv)
    }

    /// `Boolean(x)` (no `new`) — the coercion CALL form. Delegates to the
    /// engine's real `ToBoolean` (`__rtsadp_g_boolean`), never re-implementing
    /// JS truthiness here.
    ///
    /// `optional = 1`: `Boolean()` with no argument is legal JS and is `false`,
    /// so the trailing slot pads with `undefined` and ToBoolean does the rest.
    #[rtse::functioncall(optional = 1)]
    fn call(value: Poly) -> Poly {
        unsafe { __rtsadp_g_boolean(value) }
    }
}
