//! `Object.prototype`'s instance surface as a pure-Rust value-class.
//!
//! Mirrors the pattern `String`/`Boolean`/`Number` proved
//! (`string/value_class.rs`, `boolean.rs`, `number/mod.rs`): no `.ts` prelude,
//! no hand-written `extern "C"` symbol — the macro declares the members and the
//! baker links them.
//!
//! `Object` has no autobox wrapper: a `{}` is ALREADY a shape-based object, so
//! there is no `__prim` slot and no dual-`this` unwrap the way `Boolean` needs.
//! The `value` flavour is still the correct one — it makes each method take the
//! receiver as a raw `Poly` word, which is exactly what a shaped-object receiver
//! is on the wire.
//!
//! Construction is NOT here: `Object(x)` and `new Object(x)` route to the
//! `__rtsadp_obj_factory` trampoline, because they must yield a plain shaped
//! object, never an `Entry::Rtse` struct. That absence is load-bearing —
//! `front/run/globalclass.rs::registry_class_is_constructible` decides
//! constructibility by asking whether a registered class carries a ctor, so an
//! instance-only class like this one is routed to the factory by DATA rather
//! than by a name check.

use rts_engine::abi::ty::Poly;

// The three PROTOCOL members are answered by the engine itself on the fast path
// (`front/run/method.rs::try_object_protocol_method`, which runs before class
// dispatch). They are declared here anyway — see the module doc — and delegate
// to those exact trampolines rather than re-implementing them, so there is ONE
// behaviour, not two that can drift. Defined in `rts-runtime` (above this
// crate) and reached by a forward decl; layering-safe because `rts-primitives`
// is an rlib and the symbol resolves at the final link, the same pattern
// `string/value_class.rs` uses for `__rtsadp_to_string`.
unsafe extern "C" {
    fn __rtsadp_has_own(obj: u64, key: u64) -> u64;
    fn __rtsadp_prop_is_enumerable(obj: u64, key: u64) -> u64;
    fn __rtsadp_is_prototype_of(proto: u64, obj: u64) -> u64;
}

/// The receiver type for the `Object` value-class.
///
/// Carries the `#[rtse::class]` attribute even though it has no
/// `#[rtse::variable]` fields: the macro's two halves coordinate through a
/// generated `__rtse_fields_OBJECT`, which only the struct form emits.
#[rtse::class("Object", value)]
#[derive(Clone)]
pub struct ObjectValue;

#[rtse::class("Object", value)]
impl ObjectValue {
    /// `Object.prototype.toString()` — the default tag for a plain object.
    #[rtse::method]
    fn to_string(_recv: Poly) -> String {
        "[object Object]".to_string()
    }

    /// `Object.prototype.toLocaleString()` — the spec delegates to `toString`,
    /// and for a plain object that is the same tag.
    #[rtse::method]
    fn to_locale_string(_recv: Poly) -> String {
        "[object Object]".to_string()
    }

    /// `Object.prototype.valueOf()` — the object itself, unchanged.
    #[rtse::method]
    fn value_of(recv: Poly) -> Poly {
        recv
    }

    /// `obj.hasOwnProperty(key)` — delegates to the engine's own own-slot check.
    #[rtse::method]
    fn has_own_property(recv: Poly, key: Poly) -> Poly {
        unsafe { __rtsadp_has_own(recv, key) }
    }

    /// `obj.propertyIsEnumerable(key)` — own AND enumerable.
    #[rtse::method]
    fn property_is_enumerable(recv: Poly, key: Poly) -> Poly {
        unsafe { __rtsadp_prop_is_enumerable(recv, key) }
    }

    /// `proto.isPrototypeOf(obj)` — the prototype-chain walk.
    #[rtse::method]
    fn is_prototype_of(recv: Poly, obj: Poly) -> Poly {
        unsafe { __rtsadp_is_prototype_of(recv, obj) }
    }
}
