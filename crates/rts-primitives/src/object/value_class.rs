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

    // The STATIC surface. Each of these is the exact trampoline
    // `front/run/objstatic.rs` already emits for the CALLED form
    // (`Object.keys(o)`), so declaring the statics here adds a Registry row over
    // the SAME implementation — it does not fork one.
    fn __rtsadp_obj_keys(o: u64) -> u64;
    fn __rtsadp_obj_values(o: u64) -> u64;
    fn __rtsadp_obj_entries(o: u64) -> u64;
    fn __rtsadp_obj_own_names(o: u64) -> u64;
    fn __rtsadp_obj_own_symbols(o: u64) -> u64;
    fn __rtsadp_obj_assign(target: u64, source: u64) -> u64;
    fn __rtsadp_obj_from_entries(entries: u64) -> u64;
    fn __rtsadp_obj_create(proto: u64) -> u64;
    fn __rtsadp_obj_proto_of(o: u64) -> u64;
    fn __rtsadp_obj_set_proto(o: u64, proto: u64) -> u64;
    fn __rtsadp_obj_define_property(o: u64, k: u64, d: u64) -> u64;
    fn __rtsadp_obj_define_properties(o: u64, d: u64) -> u64;
    fn __rtsadp_obj_get_own_property_descriptor(o: u64, k: u64) -> u64;
    fn __rtsadp_obj_get_own_property_descriptors(o: u64) -> u64;
    fn __rtsadp_obj_group_by(items: u64, cb: u64) -> u64;
    fn __rtsadp_freeze(o: u64) -> u64;
    fn __rtsadp_seal(o: u64) -> u64;
    fn __rtsadp_is_frozen(o: u64) -> u64;
    fn __rtsadp_is_sealed(o: u64) -> u64;
    fn __rtsadp_is_extensible(o: u64) -> u64;
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

    // ── STATICS ──────────────────────────────────────────────────────────────
    //
    // These replace the `object.ts` prelude block, the last `.ts` remnant of
    // `Object`. Each delegates to the trampoline `objstatic.rs` already emits
    // for the CALLED form, so there is one implementation with a Registry row
    // over it, never a second copy.
    //
    // NOTE ON THE READ-AS-A-VALUE FORM (`const f = Object.keys`): the generic
    // class-static reader consults `desc.statics`, which only an AMBIENT `.ts`
    // class populates — there is no path yet that reads a REGISTRY class static
    // as a value, for ANY class (`Number.isNaN` fails the same way). Declaring
    // them here is the prerequisite for that path, not a substitute for it; the
    // read form regresses until it is built. The CALLED form is unaffected —
    // `objstatic.rs` intercepts it before class dispatch.

    #[rtse::statical]
    fn keys(o: Poly) -> Poly {
        unsafe { __rtsadp_obj_keys(o) }
    }

    #[rtse::statical]
    fn values(o: Poly) -> Poly {
        unsafe { __rtsadp_obj_values(o) }
    }

    #[rtse::statical]
    fn entries(o: Poly) -> Poly {
        unsafe { __rtsadp_obj_entries(o) }
    }

    #[rtse::statical(name = "getOwnPropertyNames")]
    fn get_own_property_names(o: Poly) -> Poly {
        unsafe { __rtsadp_obj_own_names(o) }
    }

    #[rtse::statical(name = "getOwnPropertySymbols")]
    fn get_own_property_symbols(o: Poly) -> Poly {
        unsafe { __rtsadp_obj_own_symbols(o) }
    }

    #[rtse::statical]
    fn assign(target: Poly, source: Poly) -> Poly {
        unsafe { __rtsadp_obj_assign(target, source) }
    }

    #[rtse::statical(name = "fromEntries")]
    fn from_entries(entries: Poly) -> Poly {
        unsafe { __rtsadp_obj_from_entries(entries) }
    }

    #[rtse::statical]
    fn create(proto: Poly) -> Poly {
        unsafe { __rtsadp_obj_create(proto) }
    }

    #[rtse::statical(name = "getPrototypeOf")]
    fn get_prototype_of(o: Poly) -> Poly {
        unsafe { __rtsadp_obj_proto_of(o) }
    }

    #[rtse::statical(name = "setPrototypeOf")]
    fn set_prototype_of(o: Poly, proto: Poly) -> Poly {
        unsafe { __rtsadp_obj_set_proto(o, proto) }
    }

    #[rtse::statical(name = "defineProperty")]
    fn define_property(o: Poly, k: Poly, d: Poly) -> Poly {
        unsafe { __rtsadp_obj_define_property(o, k, d) }
    }

    #[rtse::statical(name = "defineProperties")]
    fn define_properties(o: Poly, d: Poly) -> Poly {
        unsafe { __rtsadp_obj_define_properties(o, d) }
    }

    #[rtse::statical(name = "getOwnPropertyDescriptor")]
    fn get_own_property_descriptor(o: Poly, k: Poly) -> Poly {
        unsafe { __rtsadp_obj_get_own_property_descriptor(o, k) }
    }

    #[rtse::statical(name = "getOwnPropertyDescriptors")]
    fn get_own_property_descriptors(o: Poly) -> Poly {
        unsafe { __rtsadp_obj_get_own_property_descriptors(o) }
    }

    #[rtse::statical(name = "groupBy")]
    fn group_by(items: Poly, cb: Poly) -> Poly {
        unsafe { __rtsadp_obj_group_by(items, cb) }
    }

    #[rtse::statical]
    fn freeze(o: Poly) -> Poly {
        unsafe { __rtsadp_freeze(o) }
    }

    #[rtse::statical]
    fn seal(o: Poly) -> Poly {
        unsafe { __rtsadp_seal(o) }
    }

    #[rtse::statical(name = "isFrozen")]
    fn is_frozen(o: Poly) -> Poly {
        unsafe { __rtsadp_is_frozen(o) }
    }

    #[rtse::statical(name = "isSealed")]
    fn is_sealed(o: Poly) -> Poly {
        unsafe { __rtsadp_is_sealed(o) }
    }

    #[rtse::statical(name = "isExtensible")]
    fn is_extensible(o: Poly) -> Poly {
        unsafe { __rtsadp_is_extensible(o) }
    }
}
