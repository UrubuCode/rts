//! `util.types` — the brand predicates, and the ones this engine cannot answer.
//!
//! # Why this namespace is three members and not forty
//!
//! Every `util.types.*` predicate is a BRAND check: `isMap` is true for a real
//! `Map` and false for an object that merely has `get`/`set`/`size`, and that
//! distinction is the entire reason the namespace exists — a program reaching
//! for it has already decided that `typeof`, `instanceof` and duck-typing are
//! not enough.
//!
//! The host surface (`rts-core-rwk`'s `entry`, read in full) exposes exactly
//! three brands: `is_array` (the array side table), `bytes_of` (the buffer view
//! table, which is `Some` for precisely a `TypedArray` or a `DataView`) and the
//! module specifier table. Nothing else is reachable — a host module cannot read
//! a global by name, so it cannot get `Map`, `Set`, `Date`, `RegExp`, `Promise`
//! or `Error` to hand to `instance_of`, and `class_support`'s prototype registry
//! is idempotent BY NAME, so asking it for `"Map"` would MINT an empty prototype
//! and win the name from the real class.
//!
//! The rejected alternative was reading `value.constructor.name` and comparing
//! strings. It compiles, it looks right, and it is a coercion used as a type
//! test: it answers true for `{ constructor: { name: "Map" } }` and false for a
//! subclass of `Map`. That is the exact defect class this repository has already
//! shipped three times, so the rest of the namespace is refused by name instead
//! and the missing capability is a host one.

use rts_core_rwk::entry;

use super::values::bool_value;

/// The members this engine can answer honestly.
pub(super) const MEMBERS: &[(&str, entry::Provided)] = &[
    ("isArray", super::legacy::is_array),
    ("isArrayBufferView", is_array_buffer_view),
    ("isModuleNamespaceObject", is_module_namespace_object),
];

/// `util.types.isArrayBufferView(value)` — any `TypedArray`, or a `DataView`.
///
/// Exact rather than inferred: `bytes_of` answers `Some` for a value the
/// runtime's buffer-view table knows and `None` for everything else, and "has a
/// view" IS the definition of `ArrayBuffer.isView`. It costs a copy of the
/// bytes, which is `bytes_of`'s own documented price and the reason this is the
/// only view predicate here rather than the cheap one it looks like.
extern "C" fn is_array_buffer_view(
    _e: u64,
    _this: u64,
    value: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let held = entry::with_runtime(|context| entry::bytes_of(context, value).is_some());
    bool_value(held)
}

/// `util.types.isModuleNamespaceObject(value)` — what `import * as ns` binds.
///
/// Identity against the specifier table, which is the only registry of these
/// objects there is: a namespace is a plain object, so no shape test could
/// distinguish one, but every one of them is reachable from
/// `module_specifiers` — including the ones a compiled module published for
/// itself through `module_publish`, which no static list of host modules has.
extern "C" fn is_module_namespace_object(
    _e: u64,
    _this: u64,
    value: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let held = entry::with_runtime(|context| {
        entry::is_object(context, value)
            && entry::module_specifiers(context)
                .iter()
                .any(|specifier| entry::module_at_name(context, specifier) == value)
    });
    bool_value(held)
}
