//! Argument/option reading shared by [`super::sync_ops`] and
//! [`super::spawn_async`] — the same small vocabulary [`super::path`] and
//! [`crate::fs`] already use (`text`/`string`/`option_flag`), plus the
//! array- and object-walking pair `own_keys` + `get_indexed` that
//! [`crate::util`]'s `format_value` established for reading a JS array/object
//! from a native without a second borrow of the runtime.

use rts_core_rwk::entry;

/// An argument as text, `None` when it is `undefined`.
///
/// Also where every native in this module delivers its due
/// [`super::spawn_async`] events — see that module's doc for exactly when a
/// listener fires. [`crate::fs`]'s own `text`/`number` pair pumps
/// `crate::fs::watch` the same way, for the same reason: every member here
/// reads at least one argument through this before doing anything else.
pub(super) fn text(value: u64) -> Option<String> {
    super::spawn_async::pump();
    let absent = entry::undefined_value();
    match value == absent {
        true => None,
        false => entry::text_of(value),
    }
}

/// A string value.
pub(super) fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// A boolean value.
pub(super) fn bool_value(held: bool) -> u64 {
    entry::boolean_value(held)
}

/// One member of an options object, `None` when `options` itself is absent
/// or does not carry `name`.
fn option_member(options: u64, name: &str) -> Option<u64> {
    let absent = entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, name));
    match value == absent {
        true => None,
        false => Some(value),
    }
}

/// A text option.
pub(super) fn option_text(options: u64, name: &str) -> Option<String> {
    option_member(options, name).and_then(text)
}

/// A numeric option.
pub(super) fn option_number(options: u64, name: &str) -> Option<f64> {
    option_member(options, name).and_then(entry::number_of)
}

/// The raw member value, for a shape [`option_text`]/[`option_number`] would
/// throw away — `env` (an object) and `stdio` (a string or an array).
pub(super) fn option_value(options: u64, name: &str) -> Option<u64> {
    option_member(options, name)
}

/// Every element of a JS array, by index — [`crate::util`]'s
/// `array_items`/`own_key_strings` pattern, restated here rather than made
/// `pub(crate)` there: this module reads `command`'s `args` array and
/// `env`'s key list, and reaching across to a sibling module for four lines
/// would trade one small duplication for a `pub(crate)` surface neither
/// module otherwise needs.
fn length_of(array_value: u64) -> usize {
    let length_key = string("length");
    entry::number_of(entry::get_indexed(array_value, length_key)).unwrap_or(0.0).max(0.0) as usize
}

fn array_items(array_value: u64) -> Vec<u64> {
    let length = length_of(array_value);
    (0..length)
        .map(|index| entry::get_indexed(array_value, entry::make_number(index as f64)))
        .collect()
}

/// `args`, as `String`s — a value that is not an array (including `undefined`)
/// reads as no arguments at all, matching Node's own default.
pub(super) fn string_array(value: u64) -> Vec<String> {
    if !entry::is_array(value) {
        return Vec::new();
    }
    array_items(value).into_iter().filter_map(text).collect()
}

/// `options.env`'s own keys, as text — the read half of an env object a
/// caller built with `{ FOO: "bar" }`.
pub(super) fn own_key_strings(value: u64) -> Vec<String> {
    let keys_array = entry::own_keys(value);
    array_items(keys_array).into_iter().filter_map(|key| entry::described(key)).collect()
}
