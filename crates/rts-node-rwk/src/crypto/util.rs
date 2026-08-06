//! Argument readers shared by every member in this module — the same role
//! `fs/mod.rs`'s `text`/`string`/`number` play there, kept local because this
//! crate's modules do not reach into one another's private helpers.

use rts_core_rwk::entry::{self, Context};

/// An argument as bytes, Node's `BinaryLike` coercion: a `Buffer`/typed array
/// crosses through [`entry::bytes_of`] unchanged; a string is UTF-8-encoded
/// (matching Node's default when no `inputEncoding` is given). Absent or any
/// other value answers empty, not `None` — every caller here wants bytes to
/// feed a hasher/KDF, and "no bytes" is the honest empty-input case rather
/// than a distinguishable error a native cannot throw anyway.
pub(super) fn binary_bytes(context: &Context, value: u64) -> Vec<u8> {
    if let Some(bytes) = entry::bytes_of(context, value) {
        return bytes;
    }
    entry::text_in(context, value).map(String::into_bytes).unwrap_or_default()
}

/// The same coercion, honoring an explicit `inputEncoding` when `value` is a
/// string — Node applies `inputEncoding` only to a string `data` argument,
/// never to a `Buffer`/typed array, which already carries real bytes.
pub(super) fn binary_bytes_encoded(context: &Context, value: u64, encoding: Option<&str>) -> Vec<u8> {
    if let Some(bytes) = entry::bytes_of(context, value) {
        return bytes;
    }
    let Some(text) = entry::text_in(context, value) else {
        return Vec::new();
    };
    match encoding.and_then(entry::canonical_encoding) {
        Some(name) => entry::encode_text(&text, name).unwrap_or_default(),
        None => text.into_bytes(),
    }
}

/// A string argument, `None` when absent or not a string.
pub(super) fn text(context: &Context, value: u64) -> Option<String> {
    entry::text_in(context, value)
}

/// A finite `f64` argument as an integer, `None` when absent or not a
/// number.
pub(super) fn integer(context: &Context, value: u64) -> Option<i64> {
    let _ = context;
    entry::number_of(value).map(|value| value as i64)
}

/// Bytes as a `StrPtr`-shaped output: the requested encoding when one was
/// given (a `string` answer), or a raw byte answer (a `Uint8Array` — see
/// this module's own doc for why not a `Buffer` instance) when none was.
/// Mirrors `node:fs`'s own `readFileSync` divergence rather than inventing a
/// second one.
pub(super) fn digest_output(context: &mut Context, bytes: &[u8], encoding: Option<&str>) -> u64 {
    match encoding.and_then(entry::canonical_encoding) {
        Some(name) => {
            let text = entry::decode_bytes(bytes, name);
            entry::make_string(context, &text)
        }
        // A `Buffer`, which is what Node answers and what `Buffer.isBuffer` and
        // every instance method need — a `Uint8Array` would answer false to the
        // first and have none of the second.
        None => entry::make_buffer(context, bytes),
    }
}

/// A named property read off `this`.
///
/// Through `get_member`, which takes the context, and NOT through
/// `get_indexed`, which is an entry point that takes the ambient borrow. Every
/// caller here is already inside `with_runtime`, so the ambient form is a
/// nested borrow — an abort rather than an error, and this is where it fired.
pub(super) fn hidden_number(context: &mut Context, this: u64, name: &str) -> Option<u64> {
    let value = entry::get_member(context, this, name);
    entry::number_of(value).map(|value| value as u64)
}

/// Stores a named number property on an instance being built.
pub(super) fn put_hidden_number(context: &mut Context, instance: u64, name: &str, value: u64) {
    let number = entry::make_number(value as f64);
    entry::put_member(context, instance, name, number);
}
