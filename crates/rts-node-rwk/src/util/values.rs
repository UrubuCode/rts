//! Reading an argument, and answering with a value.
//!
//! # Why every reader here is AMBIENT and every builder takes a context
//!
//! The two halves of this module obey opposite rules, and mixing them is the
//! defect this crate fears most. `own_keys`, `get_indexed`, `type_of` and
//! `described` take the runtime borrow themselves, so a caller must hold none —
//! an `extern "C"` frame cannot unwind past the re-entrant-borrow panic and the
//! process ABORTS. `make_object`, `put_member`, `make_array_in` and
//! `make_string` take a `&mut Context`, so a caller must hold one.
//!
//! So the shape every native in this module tree has is: **read ambiently,
//! compute in Rust, then build inside ONE borrow**. The readers below take no
//! context by design — a helper that can only be called correctly beats one a
//! caller has to remember the rule for.

use rts_core_rwk::entry;

/// A string value, from outside a borrow.
pub(super) fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// A boolean value.
pub(super) fn bool_value(held: bool) -> u64 {
    entry::boolean_value(held)
}

/// `typeof value`, as Rust text.
pub(super) fn kind_of(value: u64) -> String {
    let kind = entry::type_of(value);
    entry::described(kind).unwrap_or_default()
}

/// The text a value holds **only when it really is a string**.
///
/// Not `described`, which is `ToString` and answers `"42"` for a number: this
/// module asks WHAT an argument is far more often than it converts one, and a
/// coercion standing in for a type test is the defect that made
/// `node:worker_threads` cross every number as text.
pub(super) fn string_of(value: u64) -> Option<String> {
    entry::with_runtime(|context| entry::string_in(context, value))
}

/// The number a value holds, only when it really holds one.
pub(super) fn number_of(value: u64) -> Option<f64> {
    entry::number_of(value)
}

/// Whether an argument was passed at all.
pub(super) fn present(value: u64) -> bool {
    value != entry::undefined_value()
}

/// Whether a value is an object a property can be read off.
pub(super) fn is_object_value(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_object(context, value))
}

/// Whether a value can be called.
pub(super) fn is_callable(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_callable_in(context, value))
}

/// One property, by name, from outside a borrow.
///
/// `get_indexed` rather than `get_member`, because `get_member` interns a NAME
/// key and an array's elements are not stored under one — the same read has to
/// answer `o.depth` and `a[0]`, and only the indexed form does both.
pub(super) fn get(object: u64, key: &str) -> u64 {
    entry::get_indexed(object, string(key))
}

/// One property of an options bag, `undefined` when there is no bag.
pub(super) fn option(options: u64, name: &str) -> u64 {
    match is_object_value(options) {
        true => get(options, name),
        false => entry::undefined_value(),
    }
}

/// A value's own enumerable keys, as Rust text.
pub(super) fn own_key_strings(value: u64) -> Vec<String> {
    array_items(entry::own_keys(value))
        .into_iter()
        .filter_map(|key| entry::described(key))
        .collect()
}

/// An array value's `length`, as a Rust count.
pub(super) fn length_of(array_value: u64) -> usize {
    // Asked of the value, not of its text: reading a number by parsing its
    // decimal back is lossy where a double's shortest decimal is not the double.
    entry::number_of(get(array_value, "length"))
        .map(|count| count as usize)
        .unwrap_or(0)
}

/// Every element of an array value, read by index.
pub(super) fn array_items(array_value: u64) -> Vec<u64> {
    (0..length_of(array_value)).map(|index| get(array_value, &index.to_string())).collect()
}

/// Every element of an array argument as text, dropping anything that is not a
/// string.
///
/// Dropping rather than coercing: every caller here — `parseArgs`'s `args`,
/// `styleText`'s format list — is given a list of strings by contract, and a
/// number silently becoming `"3"` would be the coercion-as-type-test defect
/// again, one layer down.
pub(super) fn string_items(array_value: u64) -> Vec<String> {
    array_items(array_value).into_iter().filter_map(string_of).collect()
}
