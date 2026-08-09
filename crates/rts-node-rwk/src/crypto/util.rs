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

/// The same coercion, from OUTSIDE the borrow, and reading a plain array too.
///
/// # Why there are two of these
///
/// Not a duplicate: it is the same coercion asked in the one place where an
/// ARRAY can be read. An array's elements are not properties — `get_member` on
/// `"4"` answers `undefined` for a five-element array, because elements live
/// beside the cell — and the only public reader of one is
/// [`entry::get_indexed`], which is an ambient entry point that takes its own
/// borrow. Calling it from inside [`entry::with_runtime`] is a nested borrow,
/// which aborts the process rather than failing.
///
/// So a caller that needs array input reads it BEFORE opening the borrow, the
/// same discipline `assert/mod.rs` states for its options reads. Accepting a
/// plain `number[]` at all is a divergence — Node's `BinaryLike` is
/// `string | ArrayBufferView` — and the alternative is worse than the
/// divergence: without it an array hashed as the EMPTY input and answered a
/// digest that looked real.
///
/// Each element is truncated to a byte the way a `Uint8Array` store is, rather
/// than refused, because there is no way to report a bad element that a caller
/// could tell apart from empty input.
pub(super) fn binary_like(value: u64) -> Vec<u8> {
    if let Some(bytes) = entry::with_runtime(|context| entry::bytes_of(context, value)) {
        return bytes;
    }
    if let Some(elements) = elements(value) {
        return elements
            .into_iter()
            .map(|held| entry::number_of(held).unwrap_or(0.0) as i64 as u8)
            .collect();
    }
    entry::with_runtime(|context| entry::text_in(context, value))
        .map(String::into_bytes)
        .unwrap_or_default()
}

/// Every element of an array value, or `None` when it is not one.
///
/// Ambient, and each borrow is opened and closed before the next: the length is
/// a property (so `get_member` inside a borrow answers it) and the elements are
/// not (so `get_indexed` outside one is what reads them).
fn elements(value: u64) -> Option<Vec<u64>> {
    let count = entry::with_runtime(|context| {
        if !entry::is_array_in(context, value) {
            return None;
        }
        let length = entry::get_member(context, value, "length");
        Some(entry::number_of(length).unwrap_or(0.0).max(0.0) as usize)
    })?;
    Some((0..count).map(|index| entry::get_indexed(value, entry::make_number(index as f64))).collect())
}

/// An argument past the fourth, from outside the borrow.
///
/// # Why this is not a fifth parameter
///
/// It cannot be: a native has the calling convention every compiled function
/// has — `entry::ARGUMENT_SLOTS` is four — and a fifth slot would be a second
/// convention. A call with more arguments already puts them in a vector the
/// runtime holds for the activation, and [`entry::rest_arguments`] is what
/// reads it; this asks that rather than inventing a second answer to "what was
/// this call given".
///
/// `undefined` when the caller supplied four or fewer — `rest_arguments`
/// answers an empty array for a vectorless activation, which is exactly the
/// case. Two `node:crypto` functions are five-argument in Node (`pbkdf2Sync`,
/// `hkdfSync`); before this each dropped its last argument, which made
/// `hkdfSync` read `keylen` from `info` and derive nothing.
pub(super) fn extra_argument(index: usize, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let rest = entry::rest_arguments(index as i64, a0, a1, a2, a3);
    entry::get_indexed(rest, entry::make_number(0.0))
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
///
/// [`entry::string_in`] and not `text_in`: the latter is `ToString`, so an
/// ABSENT argument answered `Some("undefined")` — and every caller here treats
/// a name it does not recognize as "no encoding given", so
/// `crypto.hash("sha256", "abc")` took the same branch as an explicit
/// `'buffer'` and answered bytes where Node answers hex. A coercion that can
/// be mistaken for a test will be, which is the rule `string_in`'s own doc
/// states.
pub(super) fn text(context: &Context, value: u64) -> Option<String> {
    entry::string_in(context, value)
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
