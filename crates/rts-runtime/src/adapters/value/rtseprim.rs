//! `ToPrimitive` for a VALUE-CLASS WRAPPER object — `new Number(5)`,
//! `new String(s)`, `new Boolean(b)`.
//!
//! Such a wrapper is an opaque `Entry::Rtse` carrying its class tag; its
//! `valueOf`/`toString` are REGISTERED members of that
//! `#[rtse::class(.., value)]`, not object slots. The keyed-object walk in
//! [`super::genops::to_primitive_via_method`] reads slots, so on such a receiver
//! it found nothing and EVERY coercion fell through to `[object Object]` / `NaN`
//! (`String(v)`, `v + 1`, `Number(v)`, `v.toString()` — all of them, whenever the
//! wrapper arrived as a dynamic value rather than a proven local).
//!
//! Dispatch here is by the class the VALUE carries (`rtse_class_of`) resolved
//! through the same [`super::dynci::try_runtime_ci`] table the dynamic method
//! path uses — pure data dispatch, no class named in this module.

use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;
use super::abi_adapter;

/// OrdinaryToPrimitive over a value-class wrapper: try the class's registered
/// `valueOf`/`toString` in the hint's order (`"string"` tries `toString` first;
/// every other hint tries `valueOf` first) and return the first PRIMITIVE result.
///
/// `None` when the receiver is not a value-class wrapper, the class declares
/// neither member at this arity, or every member returned a non-primitive — the
/// caller then keeps its own default coercion, never a wrong value.
pub(super) fn to_primitive(v: PolyValue, hint: &str) -> Option<PolyValue> {
    if !v.is_object() {
        return None;
    }
    let h = rt_handles::__rtsn_poly_to_handle(v.as_handle());
    rts_engine::heap::handles::rtse_class_of(h)?;
    let order: [&str; 2] = if hint == "string" {
        ["toString", "valueOf"]
    } else {
        ["valueOf", "toString"]
    };
    let undef = PolyValue::undefined().raw();
    for name in order {
        let key = abi_adapter::intern_poly(name).raw();
        let Some(r) = super::dynci::try_runtime_ci(v.raw(), key, undef, undef, undef, 0) else {
            continue;
        };
        let p = PolyValue::from_raw(r);
        // A non-primitive result does not satisfy ToPrimitive — try the next
        // member (spec OrdinaryToPrimitive), else decline.
        if p.is_object() || p.is_function() {
            continue;
        }
        return Some(p);
    }
    None
}
