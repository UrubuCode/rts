//! Reading, rendering and reporting — the three things every check here does
//! that are not a comparison.
//!
//! # The borrow discipline, restated where it is paid
//!
//! Every function below either takes no borrow at all or opens exactly ONE
//! [`with_runtime`](rts_core_rwk::entry::with_runtime) and closes it before
//! returning. None calls an ambient entry point from inside one, because an
//! `extern "C"` frame cannot unwind past the nested-borrow panic and the
//! process aborts. `util.rs` states the same rule for the same reason; this is
//! the module in `assert` where it is easiest to break, since a renderer wants
//! to read a value and describe it in one pass.

use rts_core_rwk::entry;
use std::sync::atomic::{AtomicU64, Ordering};

/// How many assertions have failed since the process started.
///
/// Not exposed to a program — there is no member reading it back. It exists so
/// [`report_failure`]'s stderr line can say which failure this is, for a
/// harness scanning the log, and so two identical failures are two lines rather
/// than one repeated.
static FAILURES: AtomicU64 = AtomicU64::new(0);

/// The stderr line, counter bump, and catchable raise every failing
/// assertion produces.
///
/// The stderr line and counter predate the raise and stay: a harness scanning
/// the log, or a person reading the run, still gets the operator and both
/// values even when the raise is caught and swallowed by the caller. The
/// raise itself is `entry::throw_type_error` — `TypeError` rather than Node's
/// own `AssertionError`, since that is the only class this crate can raise
/// with publicly; see its doc in `rts-core-rwk` for why. Must be called
/// OUTSIDE any `with_runtime` borrow — raising opens its own — which every
/// caller in `checks.rs` already satisfies (see this module's doc).
pub(super) fn report_failure(operator: &str, detail: &str) {
    let count = FAILURES.fetch_add(1, Ordering::Relaxed) + 1;
    eprintln!("AssertionError [{operator}] #{count}: {detail}");
    entry::throw_type_error(detail);
}

/// The failure text: the caller's `message` when there is one, else the
/// generated one.
///
/// # Why `described` and not a type test
///
/// Because a message argument really is being CONVERTED here, not classified —
/// `assert.ok(false, 42)` reports `42`, exactly as Node's `AssertionError`
/// would carry it. The one case this loses is `message` being an `Error`, which
/// Node throws as-is rather than wrapping; there is no throw here, so that
/// distinction has nowhere to go and is refused by name in the module doc.
pub(super) fn describe_message(message: u64, generated: &str) -> String {
    if message == entry::undefined_value() {
        return generated.to_owned();
    }
    entry::described(message).unwrap_or_else(|| render(message))
}

/// A value as text for a failure message.
///
/// A one-level structural render rather than a `ToString`: [`entry::described`]
/// answers `None` for every object, so a message built from it alone would say
/// nothing about the values that actually differ — which is the entire content
/// of an assertion failure. `node:util`'s `inspect` does this properly and is
/// the obvious thing to call; its formatter is private to that module and this
/// crate's ownership rule forbids widening it from here, so this is a
/// deliberately smaller renderer that stops one level down.
pub(super) fn render(value: u64) -> String {
    render_at(value, 1)
}

/// [`render`], with the remaining depth explicit.
fn render_at(value: u64, depth: u32) -> String {
    if value == entry::undefined_value() {
        return "undefined".to_owned();
    }
    if value == entry::null_value() {
        return "null".to_owned();
    }
    match kind_of(value).as_str() {
        "string" => format!("'{}'", entry::described(value).unwrap_or_default()),
        "number" | "boolean" | "bigint" => {
            entry::described(value).unwrap_or_else(|| "NaN".to_owned())
        }
        "symbol" => "Symbol()".to_owned(),
        "function" => "[Function]".to_owned(),
        // An error before a plain object, because `name` and `message` are
        // non-enumerable: the key walk below sees neither, so `new
        // Error("boom")` would render as `{}` and `ifError`'s whole message —
        // Node's `ifError got unwanted exception: boom` — would carry nothing
        // about what went wrong. Duck-typed on a string `message`, which is
        // what `entry::error::joined` reads too; the host surface names no
        // class.
        _ if string_of(member(value, "message")).is_some() => {
            let name = string_of(member(value, "name")).unwrap_or_else(|| "Error".to_owned());
            let message = string_of(member(value, "message")).unwrap_or_default();
            format!("{name}: {message}")
        }
        _ if depth == 0 => "[Object]".to_owned(),
        _ => {
            let keys = own_key_strings(value);
            let parts: Vec<String> = keys
                .iter()
                .map(|key| {
                    let held = member(value, key);
                    format!("{key}: {}", render_at(held, depth - 1))
                })
                .collect();
            match parts.is_empty() {
                true => "{}".to_owned(),
                false => format!("{{ {} }}", parts.join(", ")),
            }
        }
    }
}

/// `typeof value`, as Rust text.
pub(super) fn kind_of(value: u64) -> String {
    entry::described(entry::type_of(value)).unwrap_or_default()
}

/// The text a value holds **only when it really is a string**.
///
/// [`entry::string_in`] rather than `text_of`, and the distinction is the one
/// this repository has already been bitten by three times: `text_of` is a
/// coercion and answers `"42"` for a number, so `assert.match(42, /4/)` would
/// pass where Node refuses the input outright.
pub(super) fn string_of(value: u64) -> Option<String> {
    entry::with_runtime(|context| entry::string_in(context, value))
}

/// Whether a value can be called.
pub(super) fn is_callable(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_callable_in(context, value))
}

/// One named member of a value.
pub(super) fn member(object: u64, name: &str) -> u64 {
    entry::get_indexed(object, string(name))
}

/// A named member, called with no arguments and `object` as the receiver.
///
/// `undefined` when the member is not callable, so a caller duck-typing a
/// `Date` or a `RegExp` gets one answer for "absent" and "not a function"
/// rather than having to ask twice.
pub(super) fn invoke(object: u64, name: &str) -> u64 {
    let absent = entry::undefined_value();
    let method = member(object, name);
    if !is_callable(method) {
        return absent;
    }
    entry::call(method, object, absent, absent, absent, absent)
}

/// A string value over Rust text.
pub(super) fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// A value's own enumerable keys, as Rust text.
pub(super) fn own_key_strings(value: u64) -> Vec<String> {
    let keys = entry::own_keys(value);
    let length = entry::number_of(member(keys, "length"))
        .map(|count| count as usize)
        .unwrap_or(0);
    (0..length)
        .filter_map(|index| entry::described(entry::get_indexed(keys, entry::make_number(index as f64))))
        .collect()
}
