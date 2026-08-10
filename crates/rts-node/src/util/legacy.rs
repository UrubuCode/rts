//! The legacy corner: `isArray`, `_extend`, `inherits`.
//!
//! Two of these are deprecated in Node (DEP0044, DEP0060) and the third is
//! Stability 3 — Legacy. They are here rather than beside the formatter because
//! they share nothing with it but a crate: each is a small operation over an
//! object's own properties, and keeping them apart is what leaves the formatter
//! room to grow toward the options it still refuses.
//!
//! Ambient throughout, per [`super::values`]'s rule.

use rts_core::entry;

use super::values::{bool_value, get, is_callable, is_object_value, own_key_strings, string};

/// `util.isArray(object)` (DEP0044) and `util.types.isArray(value)`.
///
/// The real array test rather than a shape guess — see [`super::inspect`] for
/// the divergence this removed.
pub(super) extern "C" fn is_array(_e: u64, _this: u64, value: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    bool_value(entry::is_array(value))
}

/// `util._extend(target, source)` (DEP0060) — `Object.assign` with one source.
pub(super) extern "C" fn extend(
    _e: u64,
    _this: u64,
    target: u64,
    source: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    if !is_object_value(target) || !is_object_value(source) {
        return target;
    }
    // Read ambiently first, then write ambiently: `set_indexed` takes its own
    // borrow, so collecting the pairs before writing is not an optimisation but
    // the only legal order.
    let pairs: Vec<(u64, u64)> = own_key_strings(source)
        .into_iter()
        .map(|key| (string(&key), get(source, &key)))
        .collect();
    for (key, value) in pairs {
        entry::set_indexed(target, key, value);
    }
    target
}

/// `util.inherits(constructor, superConstructor)`.
///
/// Node's own three effects, in its order: the subclass's `prototype` becomes a
/// fresh object linked to the superclass's, `constructor` points back at the
/// subclass, and `super_` names the superclass. A non-function argument is
/// answered with `undefined` rather than the `TypeError` Node throws — see the
/// module's refusal list for why nothing here throws.
pub(super) extern "C" fn inherits(
    _e: u64,
    _this: u64,
    subclass: u64,
    superclass: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    if !is_callable(subclass) || !is_callable(superclass) {
        return entry::undefined_value();
    }
    let parent = get(superclass, "prototype");
    if !is_object_value(parent) {
        return entry::undefined_value();
    }
    let made = entry::with_runtime(|context| {
        let made = entry::make_instance(context, parent);
        entry::put_member(context, made, "constructor", subclass);
        made
    });
    entry::set_indexed(subclass, string("prototype"), made);
    entry::set_indexed(subclass, string("super_"), superclass);
    entry::undefined_value()
}
