//! Argument/option reading shared by [`super::sync_ops`] and
//! [`super::spawn_async`] — the same small vocabulary [`super::path`] and
//! [`crate::fs`] already use (`text`/`string`/`option_flag`), plus the
//! array- and object-walking pair `own_keys` + `get_indexed` that
//! [`crate::util`]'s `format_value` established for reading a JS array/object
//! from a native without a second borrow of the runtime.

use rts_core::entry;

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

/// [`text`], for a caller that already holds the context.
///
/// [`text`] pumps [`super::spawn_async`]'s queue and re-enters
/// [`entry::with_runtime`] through [`entry::text_of`] — both a second borrow
/// when called from inside a `with_runtime` closure already in progress, which
/// this repository's fault is always the same shape of: an ambient helper
/// called from inside a borrow the host cannot unwind out of. This pair takes
/// the context instead, so it can only be called correctly.
pub(super) fn text_in(context: &entry::Context, value: u64) -> Option<String> {
    let absent = entry::undefined_in(context);
    match value == absent {
        true => None,
        false => entry::text_in(context, value),
    }
}

/// The text of a value that IS a string, and `None` for anything else.
///
/// # Why this is not [`text`] with a check bolted on
///
/// Because they answer different questions. [`text`] COERCES — which is right
/// for `spawn(command)`, where a program handing a number named a program with
/// a numeric name. This asks whether the program wrote a string at all, which
/// is what an option carrying two types needs: `shell: true` and
/// `shell: "/bin/sh"` mean different things, and coercion collapses them into
/// the path `"true"`.
pub(super) fn text_of_string(value: u64) -> Option<String> {
    entry::with_runtime(|context| entry::string_in(context, value))
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

/// [`option_text`], for a caller that already holds the context — see
/// [`text_in`] for why this pair exists instead of wrapping each call.
pub(super) fn option_text_in(context: &mut entry::Context, options: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    match value == absent {
        true => None,
        false => text_in(context, value),
    }
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
