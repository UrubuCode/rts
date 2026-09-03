//! The RTS wire format `v8.serialize`/`deserialize` round-trip through — NOT
//! V8's own `ValueSerializer` format. `mod.rs`'s module doc says why nothing
//! here tries to reproduce that one; this module owes the reader its own
//! shape instead, since it is a real format now rather than a copy pretending
//! to be bytes.
//!
//! # Shape
//!
//! One tag byte, then a payload only the tag decides the length of:
//!
//! | tag | value | payload |
//! |---|---|---|
//! | `0x00` | `undefined` — also a function, a symbol, or anything past [`DEPTH`] | none |
//! | `0x01` | `null` | none |
//! | `0x02` | `false` | none |
//! | `0x03` | `true` | none |
//! | `0x04` | number | 8 bytes, `f64` little-endian |
//! | `0x05` | string | `u32` LE byte length, then UTF-8 |
//! | `0x06` | array | `u32` LE element count, then that many tagged values |
//! | `0x07` | object | `u32` LE pair count, then that many (key, tagged value) pairs |
//!
//! An object's key is `u32` LE byte length then UTF-8, with **no** tag byte of
//! its own: the pair position already says it is a string, and giving it a tag
//! would spend a byte recording something the format already knows.
//!
//! # Reuse-check
//!
//! `mod.rs`'s own reuse-check names the two pieces this module is BUILT FROM:
//! [`rts_core::entry::make_buffer`] / [`rts_core::entry::bytes_of`] for the
//! byte boundary. For reading an arbitrary JS value's shape from a native, the
//! walk `rts-core`'s own `structuredClone` uses (`entry/clone.rs`) is
//! `pub(super)` — unreachable from this crate, as that module's own doc
//! states — so the shape here follows the pattern this crate already keeps for
//! exactly that gap: `crates/rts-node/src/assert/values.rs`'s
//! `own_key_strings`/`member` and `crates/rts-node/src/util/values.rs`'s
//! `own_key_strings`/`array_items`/`get` are the same recipe, each written
//! locally because the other module's copy is private to it. This is a third
//! copy of that recipe for the same reason those two are separate from each
//! other, not a new one invented here.
//!
//! # What this does NOT distinguish
//!
//! `Date`, `Map`, `Set`, `RegExp`, `Error`, `TypedArray`/`ArrayBuffer` and
//! `BigInt` all fall into the generic OBJECT arm below — own enumerable
//! string-keyed properties, nothing else. For a `Date`, a `Map`, a `Set` or a
//! `BigInt`, whose state is not an own property, that answers `{}`: the
//! value's *kind* is lost, not merely rendered less precisely. This is not a
//! blind spot invented here — `node:util`'s `inspect` formatter (checked
//! before writing this) has the identical one today — so it is consistent
//! with the rest of this crate rather than a new limit this module adds.
//! Naming a kind per special class is real future work; nothing in this
//! cluster's task asked for it and the fixture this module was written against
//! never constructs one.
//!
//! # Cycles
//!
//! There is no back-reference notation in this format, unlike
//! `structuredClone`'s own arena, which memoizes a cell to resolve one (see
//! `entry/clone.rs`'s module doc). A self-referential object does not
//! round-trip as itself here: [`write_value`] recurses until [`DEPTH`] cuts
//! the branch off, and everything past that depth encodes as `undefined`. That
//! is a real, named divergence and not a silent truncation — a back-reference
//! table is a second mechanism this cluster's task did not ask for, and
//! nothing exercised by `tests/node_v8_full.test.ts` is cyclic.

use rts_core::entry;

const TAG_UNDEFINED: u8 = 0x00;
const TAG_NULL: u8 = 0x01;
const TAG_FALSE: u8 = 0x02;
const TAG_TRUE: u8 = 0x03;
const TAG_NUMBER: u8 = 0x04;
const TAG_STRING: u8 = 0x05;
const TAG_ARRAY: u8 = 0x06;
const TAG_OBJECT: u8 = 0x07;

/// How deep [`encode`] descends before it truncates a branch to `undefined`.
///
/// Matches `rts-core`'s own `structuredClone` cap
/// (`crates/rts-core/src/entry/clone.rs::DEPTH`): the same constraint (guard
/// Rust's own stack against a genuinely deep structure — see this module's
/// "Cycles" section for why it is not what makes a cycle terminate), the same
/// number.
const DEPTH: usize = 200;

/// `value`, as wire-format bytes — see the module doc for the shape.
///
/// Ambient: every read below opens and closes its own borrow (`is_object`,
/// `string_in`, `is_callable_in` each need one), so this must not be called
/// from inside [`entry::with_runtime`] — the rule every native in this crate
/// keeps, restated here because a recursive walk is where it is easiest to
/// break by accident.
pub fn encode(value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    write_value(value, 0, &mut out);
    out
}

/// The inverse of [`encode`].
///
/// Truncated or malformed bytes decode as `undefined` at the point they stop
/// making sense, rather than panicking — the same answer this crate's entry
/// surface gives an out-of-range read anywhere else, and the only one an
/// `extern "C"` frame can survive.
///
/// One [`entry::with_runtime`] for the whole walk, not one per node:
/// `make_object`/`put_member`/`make_array_in`/`make_string` all take an
/// already-open `&mut Context` and none reaches for the ambient one
/// internally (checked in `rts-core/src/entry/modules.rs` before writing
/// this), so recursing under a single held borrow is safe — unlike [`encode`],
/// which recurses OUTSIDE one for the opposite reason: its reads are ambient.
pub fn decode(bytes: &[u8]) -> u64 {
    entry::with_runtime(|context| {
        let mut at = 0usize;
        read_value(context, bytes, &mut at)
    })
}

fn write_value(value: u64, depth: usize, out: &mut Vec<u8>) {
    if depth >= DEPTH {
        out.push(TAG_UNDEFINED);
        return;
    }
    if value == entry::undefined_value() {
        out.push(TAG_UNDEFINED);
        return;
    }
    if value == entry::null_value() {
        out.push(TAG_NULL);
        return;
    }
    if value == entry::boolean_value(false) {
        out.push(TAG_FALSE);
        return;
    }
    if value == entry::boolean_value(true) {
        out.push(TAG_TRUE);
        return;
    }
    if let Some(number) = entry::number_of(value) {
        out.push(TAG_NUMBER);
        out.extend_from_slice(&number.to_le_bytes());
        return;
    }
    // `string_in`, never `text_of`/`described`: those are `ToString` and would
    // answer `"42"` for the number 42 — already handled above, but a value this
    // format has no OTHER tag for would then silently encode as a string
    // instead of falling through to the object arm. Asking what a value IS
    // takes a predicate, not a coercion — the rule `assert/values.rs` and
    // `util/values.rs` both state for the same reason.
    if let Some(text) = entry::with_runtime(|context| entry::string_in(context, value)) {
        out.push(TAG_STRING);
        write_text(&text, out);
        return;
    }
    // A function or a symbol has no wire form: the same case
    // `structuredClone` refuses with a `DataCloneError` in its own module doc.
    // No entry point here can raise a catchable one, so the ANSWER
    // `structuredClone` gives — drop to `undefined`, keep walking the rest of
    // the structure — is reused rather than the mechanism.
    let uncallable_check = entry::with_runtime(|context| entry::is_callable_in(context, value));
    if uncallable_check {
        out.push(TAG_UNDEFINED);
        return;
    }
    if entry::is_array(value) {
        out.push(TAG_ARRAY);
        let items = array_items(value);
        out.extend_from_slice(&(items.len() as u32).to_le_bytes());
        for item in items {
            write_value(item, depth + 1, out);
        }
        return;
    }
    let is_object = entry::with_runtime(|context| entry::is_object(context, value));
    if !is_object {
        // A symbol reaches here too (it is neither a slot `is_object` accepts
        // nor caught by `is_callable_in`) — same answer, named rather than
        // given its own branch, since the outcome is identical.
        out.push(TAG_UNDEFINED);
        return;
    }
    out.push(TAG_OBJECT);
    let keys = own_key_strings(value);
    out.extend_from_slice(&(keys.len() as u32).to_le_bytes());
    for key in keys {
        write_text(&key, out);
        let held = entry::get_indexed(value, string(&key));
        write_value(held, depth + 1, out);
    }
}

fn write_text(text: &str, out: &mut Vec<u8>) {
    let bytes = text.as_bytes();
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn read_value(context: &mut entry::Context, bytes: &[u8], at: &mut usize) -> u64 {
    let Some(&tag) = bytes.get(*at) else {
        return entry::undefined_in(context);
    };
    *at += 1;
    match tag {
        TAG_NULL => entry::null_in(context),
        TAG_FALSE => entry::boolean_value(false),
        TAG_TRUE => entry::boolean_value(true),
        TAG_NUMBER => match read_bytes(bytes, at, 8) {
            Some(raw) => entry::make_number(f64::from_le_bytes(raw.try_into().unwrap_or_default())),
            None => entry::undefined_in(context),
        },
        TAG_STRING => match read_text(bytes, at) {
            Some(text) => entry::make_string(context, &text),
            None => entry::undefined_in(context),
        },
        TAG_ARRAY => {
            let Some(count) = read_u32(bytes, at) else {
                return entry::undefined_in(context);
            };
            let values = (0..count).map(|_| read_value(context, bytes, at)).collect();
            entry::make_array_in(context, values)
        }
        TAG_OBJECT => {
            let Some(count) = read_u32(bytes, at) else {
                return entry::undefined_in(context);
            };
            let object = entry::make_object(context);
            for _ in 0..count {
                let Some(key) = read_text(bytes, at) else {
                    break;
                };
                let value = read_value(context, bytes, at);
                entry::put_member(context, object, &key, value);
            }
            object
        }
        // `TAG_UNDEFINED` and anything unrecognised — an unknown tag means the
        // bytes were not this format's own, and `undefined` is the answer this
        // crate gives a read it cannot make sense of, not a guess.
        _ => entry::undefined_in(context),
    }
}

fn read_u32(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let raw = read_bytes(bytes, at, 4)?;
    Some(u32::from_le_bytes(raw.try_into().ok()?))
}

fn read_text(bytes: &[u8], at: &mut usize) -> Option<String> {
    let length = read_u32(bytes, at)? as usize;
    let raw = read_bytes(bytes, at, length)?;
    String::from_utf8(raw.to_vec()).ok()
}

fn read_bytes<'a>(bytes: &'a [u8], at: &mut usize, count: usize) -> Option<&'a [u8]> {
    let end = at.checked_add(count)?;
    let slice = bytes.get(*at..end)?;
    *at = end;
    Some(slice)
}

/// A string value, from outside a borrow — the `write`-side helper
/// [`entry::make_string`] needs a context for, built the way
/// `util/values.rs::string` and `assert/values.rs::string` both already do.
fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// An array value's `length`, as a Rust count — the same read
/// `util/values.rs::length_of` makes, kept local for the reason this module's
/// own reuse-check states.
fn length_of(array_value: u64) -> usize {
    entry::number_of(entry::get_indexed(array_value, string("length")))
        .map(|count| count as usize)
        .unwrap_or(0)
}

/// Every element of an array value, read by index.
fn array_items(array_value: u64) -> Vec<u64> {
    (0..length_of(array_value))
        .map(|index| entry::get_indexed(array_value, entry::make_number(index as f64)))
        .collect()
}

/// A value's own enumerable keys, as Rust text.
fn own_key_strings(value: u64) -> Vec<String> {
    array_items(entry::own_keys(value))
        .into_iter()
        .filter_map(entry::described)
        .collect()
}
