//! `util.types` — the brand predicates, and the ones this engine cannot answer.
//!
//! # Why this namespace used to be three members, and now is not
//!
//! Every `util.types.*` predicate is a BRAND check: `isMap` is true for a real
//! `Map` and false for an object that merely has `get`/`set`/`size`, and that
//! distinction is the entire reason the namespace exists — a program reaching
//! for it has already decided that `typeof`, `instanceof` and duck-typing are
//! not enough.
//!
//! An earlier version of this file searched `rts-core`'s host surface for a
//! brand and found exactly three: `is_array` (the array side table), `bytes_of`
//! (the buffer-view table) and the module specifier table — and concluded every
//! other predicate needed a capability the host did not expose. That search
//! stopped one layer too early. `global.rs`'s own doc says what it missed: `Map`,
//! `Set`, `Date`, `RegExp`, `Promise`, `Error`, `ArrayBuffer`, every typed array,
//! the boxed-primitive wrappers and the generator prototype are each a real
//! class this runtime builds — LAZILY, the first time something reads the bare
//! name — and [`global`] plus [`entry::instance_of`] is how code outside
//! `rts-core` already reaches one: `buffer/blob.rs`, `fs/handle.rs`,
//! `url/mod.rs` and `events/abort.rs` all do it today, for `Blob`,
//! `FileHandle`, `URL` and `AbortSignal` respectively. `is_abort_signal` in
//! `events/abort.rs` is the closest precedent — a value's prototype chain
//! checked against a global class read by name — and this file is the same
//! operation for thirty names instead of one.
//!
//! # What `instance_of` answers, and what it does not
//!
//! It is the language's own `instanceof`, not a second implementation of it —
//! see [`instance_of_global`] for the divergence from Node's internal-slot
//! check this necessarily carries, and why it is still the honest answer for
//! everything below rather than the `constructor.name` string comparison this
//! file used to reject: that alternative answers true for
//! `{ constructor: { name: "Map" } }` and false for a subclass, which
//! `instanceof` gets right on both counts.
//!
//! # What is still absent, and why each one specifically
//!
//! - **`isArgumentsObject`.** `entry::arguments.rs`'s own doc: an `arguments`
//!   value is "an ordinary object" with no class or tag distinguishing it from
//!   one a program wrote — there is nothing to check membership against.
//! - **`isAsyncFunction` / `isGeneratorFunction`.** These ask about the
//!   DECLARATION, not a value it produces. `context.callable_at` answers a code
//!   address and a captured environment and nothing else — no declared-kind bit
//!   crosses into a value a native can read.
//! - **`isMapIterator` / `isSetIterator`.** `collections/cursor.rs`'s own doc:
//!   `m.keys()` and every other built-in iterator (arrays, the ES2025 helpers)
//!   answer the SAME `ListIterator` class rather than one of their own, and the
//!   one field the cursor table does carry (key/value/entries view) does not
//!   say which COLLECTION produced it either. There is no difference to answer
//!   from — not a missing check, a missing representation.
//! - **`isProxy`.** `proxy::is_proxy` is real inside `rts-core` and is
//!   `pub(in crate::entry)` — unreachable from this crate. Worth naming even if
//!   it were reachable: a `Proxy`'s `get` trap can make it answer anything for
//!   any property a caller reads, deliberately, so a brand check built from
//!   property reads could never be more than the object's own cooperation —
//!   which is not what a brand check is for.
//! - **`isExternal`.** Node's meaning is "made by `napi_create_external`", a
//!   mechanism that exists inside `rts-napi`'s own addon boundary with no
//!   JS-visible marker crossing into `node:util`.
//! - **`isCryptoKey` / `isKeyObject`.** Neither class exists anywhere in this
//!   crate's `crypto`/`webcrypto` modules — both modules' own docs list them
//!   absent — so there is no class to test membership against, not merely a
//!   host-surface gap.

use rts_core::entry;

use super::values::bool_value;

/// One global class by name, materialised lazily the way reading the bare
/// identifier would be.
///
/// # Why not `global_object` + `get_member`
///
/// `get_member` walks an object's OWN properties, and `global.rs`'s own doc
/// says the global object carries `Map`/`Date`/`RegExp`/… only once something
/// has read the bare name — "the object is empty until something asks". A
/// program that writes `new Map()` already asked, through the compiled read
/// that reaches `global_get`, so the common case is covered either way — but
/// answering `false` for a real `Map` just because nothing else in THIS
/// program happened to read the name first would be exactly the kind of
/// plausible wrong answer this module refuses elsewhere. `entry::member_key`
/// turns the name into the same key number a compiled read would use, and
/// `entry::global_get` is the identical entry point that read reaches — so
/// asking here performs the identical lazy build rather than a weaker one.
/// `buffer/blob.rs`, `fs/handle.rs`, `url/mod.rs` and every `rts-std` global
/// module that reaches for a class by name (`fetch`, `streams`, `text`) do
/// exactly these two lines already.
fn global(name: &str) -> u64 {
    let key = entry::with_runtime(|context| i64::from(entry::member_key(context, name)));
    entry::global_get(key)
}

/// Whether `value`'s prototype chain reaches the global class `class_name`'s
/// `.prototype` — `instanceof`, spelled out for a class this module did not
/// build.
///
/// # The divergence from Node's `util.types`, named rather than hidden
///
/// Node's real predicates check an INTERNAL SLOT, which is why the reference
/// document contrasts them with `instanceof`: `util.types.isMap(x)` is false
/// for `Object.setPrototypeOf({}, Map.prototype)` even though
/// `x instanceof Map` is true for it, because the fake object was never
/// constructed as a `Map` and has no entries table behind it. This engine has
/// no public way to ask for that slot from outside `rts-core` —
/// `collections/brand.rs`'s `branded` is the real check and it is
/// `pub(in crate::entry)` — so what runs here is the prototype-chain
/// question instead.
///
/// For every value a real program hands these predicates — a `Map` it
/// constructed, a `RegExp` literal, an object another built-in returned — the
/// two questions have the same answer, because nothing manufactures a fake
/// prototype link by hand. The divergence is real only when a program
/// DELIBERATELY spoofs a prototype specifically to fool a brand check, which
/// is the scenario `util.types` exists to defend against and the one case
/// this implementation cannot.
fn instance_of_global(value: u64, class_name: &str) -> bool {
    entry::instance_of(value, global(class_name))
}

/// Generates one `util.types.isX` native that is exactly
/// [`instance_of_global`] against one global class name.
///
/// A `macro_rules!` rather than thirty near-identical functions written out,
/// for the reason `buffers/typed_classes.rs` already gives about its own
/// eight: the difference between any two of these is a name, and a written-out
/// block is where one of them ends up checking a different class than its own
/// doc comment says.
macro_rules! brand {
    ($name:ident, $js:literal, $class:literal) => {
        #[doc = concat!(
            "`util.types.", $js, "(value)` — `value instanceof ", $class,
            "`. See `instance_of_global`'s note on this file for what that ",
            "checks and the one way it can disagree with Node.",
        )]
        extern "C" fn $name(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
            bool_value(instance_of_global(value, $class))
        }
    };
}

brand!(is_map, "isMap", "Map");
brand!(is_set, "isSet", "Set");
brand!(is_weak_map, "isWeakMap", "WeakMap");
brand!(is_weak_set, "isWeakSet", "WeakSet");
brand!(is_date, "isDate", "Date");
brand!(is_reg_exp, "isRegExp", "RegExp");
brand!(is_promise, "isPromise", "Promise");
// Every built-in error subclass (`TypeError`, `RangeError`, …) is registered
// through the `extends = register_error` the class attribute reads, so its
// prototype chains to `Error.prototype` and this one check covers all seven —
// see `error.rs`.
brand!(is_native_error, "isNativeError", "Error");
brand!(is_array_buffer, "isArrayBuffer", "ArrayBuffer");
brand!(is_shared_array_buffer, "isSharedArrayBuffer", "SharedArrayBuffer");
brand!(is_data_view, "isDataView", "DataView");
brand!(is_int8_array, "isInt8Array", "Int8Array");
brand!(is_uint8_array, "isUint8Array", "Uint8Array");
brand!(is_uint8_clamped_array, "isUint8ClampedArray", "Uint8ClampedArray");
brand!(is_int16_array, "isInt16Array", "Int16Array");
brand!(is_uint16_array, "isUint16Array", "Uint16Array");
brand!(is_int32_array, "isInt32Array", "Int32Array");
brand!(is_uint32_array, "isUint32Array", "Uint32Array");
brand!(is_float32_array, "isFloat32Array", "Float32Array");
brand!(is_float64_array, "isFloat64Array", "Float64Array");
brand!(is_big_int64_array, "isBigInt64Array", "BigInt64Array");
brand!(is_big_uint64_array, "isBigUint64Array", "BigUint64Array");
// `generator::made` sets every generator object's prototype to the SAME cell
// `class_support::prototype(context, "Generator")` answers — see that
// function's own comment — so a real generator's chain reaches this global's
// `.prototype` exactly as a `Map`'s reaches `Map`'s.
brand!(is_generator_object, "isGeneratorObject", "Generator");
brand!(is_boolean_object, "isBooleanObject", "Boolean");
brand!(is_number_object, "isNumberObject", "Number");
brand!(is_string_object, "isStringObject", "String");
brand!(is_symbol_object, "isSymbolObject", "Symbol");
brand!(is_big_int_object, "isBigIntObject", "BigInt");

/// `util.types.isAnyArrayBuffer(value)` — an `ArrayBuffer` or a
/// `SharedArrayBuffer`.
extern "C" fn is_any_array_buffer(
    _e: u64,
    _this: u64,
    value: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    bool_value(
        instance_of_global(value, "ArrayBuffer") || instance_of_global(value, "SharedArrayBuffer"),
    )
}

/// The eleven concrete typed-array kinds, walked by [`is_typed_array`] and
/// nowhere else — a class list rather than a shared `%TypedArray%` check,
/// because this engine gives each of the eight numeric kinds its own
/// prototype rather than a common one (`typed_classes.rs`'s own "What is not
/// here"), so there is no single global to check `instanceof` against.
const TYPED_ARRAY_CLASSES: &[&str] = &[
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
];

/// `util.types.isTypedArray(value)` — any of the eleven.
extern "C" fn is_typed_array(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    bool_value(TYPED_ARRAY_CLASSES.iter().any(|class| instance_of_global(value, class)))
}

/// The five boxed-primitive wrappers, walked by [`is_boxed_primitive`].
const BOXED_PRIMITIVE_CLASSES: &[&str] = &["Boolean", "Number", "String", "Symbol", "BigInt"];

/// `util.types.isBoxedPrimitive(value)` — `new Boolean`/`Number`/`String`, or
/// the two `Object(x)` builds by hand: `Object(Symbol())` and `Object(1n)`.
/// `primitive_proto.rs`'s `wrap` is the first three and `object_global/mod.rs`'s
/// `make` is all five — see that function's own doc for why `Object(x)` boxes
/// a symbol and a bigint where `new Symbol`/`new BigInt` cannot: this engine
/// does not refuse `new` on either, so it answers the unboxed primitive
/// instead of throwing, and `Object(x)` is the only path that reaches a boxed
/// one of those two.
extern "C" fn is_boxed_primitive(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    bool_value(BOXED_PRIMITIVE_CLASSES.iter().any(|class| instance_of_global(value, class)))
}

/// The members this engine can answer honestly.
pub(super) const MEMBERS: &[(&str, entry::Provided)] = &[
    ("isArray", super::legacy::is_array),
    ("isArrayBufferView", is_array_buffer_view),
    ("isModuleNamespaceObject", is_module_namespace_object),
    ("isAnyArrayBuffer", is_any_array_buffer),
    ("isArrayBuffer", is_array_buffer),
    ("isSharedArrayBuffer", is_shared_array_buffer),
    ("isDataView", is_data_view),
    ("isTypedArray", is_typed_array),
    ("isInt8Array", is_int8_array),
    ("isUint8Array", is_uint8_array),
    ("isUint8ClampedArray", is_uint8_clamped_array),
    ("isInt16Array", is_int16_array),
    ("isUint16Array", is_uint16_array),
    ("isInt32Array", is_int32_array),
    ("isUint32Array", is_uint32_array),
    ("isFloat32Array", is_float32_array),
    ("isFloat64Array", is_float64_array),
    ("isBigInt64Array", is_big_int64_array),
    ("isBigUint64Array", is_big_uint64_array),
    ("isBooleanObject", is_boolean_object),
    ("isNumberObject", is_number_object),
    ("isStringObject", is_string_object),
    ("isSymbolObject", is_symbol_object),
    ("isBigIntObject", is_big_int_object),
    ("isBoxedPrimitive", is_boxed_primitive),
    ("isDate", is_date),
    ("isMap", is_map),
    ("isSet", is_set),
    ("isWeakMap", is_weak_map),
    ("isWeakSet", is_weak_set),
    ("isPromise", is_promise),
    ("isRegExp", is_reg_exp),
    ("isNativeError", is_native_error),
    ("isGeneratorObject", is_generator_object),
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
