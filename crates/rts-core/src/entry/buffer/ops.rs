//! What every `Buffer` member does, over [`super::codec`] and
//! [`super::super::buffers`] — the same split [`super::super::buffers::typed`]
//! is written over, and for the same reason: eight (here, one) declarations in
//! [`super::mod`] stay one line each, and the borrow discipline lives in
//! exactly one place.
//!
//! # Where the borrow is taken
//!
//! Every member takes exactly one, at the top, and drops it before answering.
//! An argument needing "was this omitted" — a begin/end/offset pair — is
//! converted through [`super::super::buffers::optional_number`] BEFORE the
//! borrow opens, because that helper (and [`super::super::class_support::to_number`]
//! beneath it) takes one of its own: calling it from inside an open borrow is
//! the second-borrow abort every native here is written to avoid.
//!
//! # What is next door
//!
//! The `Buffer.*` statics moved to [`super::statics`] when Node's argument
//! refusals took this file past the crate's 500-line ceiling — see that module
//! for the seam. The helpers below are shared by both halves rather than copied,
//! which is why three of them are `pub(in crate::entry)` and the rest are not.
//! What an argument has to BE is [`super::validate`]; this file decides only
//! what to do once it is.

use super::super::buffers::element::Kind;
use super::super::buffers::{
    View, as_count, optional_number, range, undefined, view_of, window, window_mut,
};
use super::super::errors;
use super::super::objects::undefined_of;
use super::super::{Context, native, with_current};
use super::codec;
use crate::entry;
use super::validate::{self, Shape};
use crate::value::Value;

/// A new `Buffer` instance, owning a copy of these bytes.
pub(in crate::entry) fn made(context: &mut Context, bytes: &[u8]) -> u64 {
    let Some(buffer) = super::super::buffers::new_buffer(context, bytes.len()) else {
        return undefined_of(context);
    };
    let view = View {
        buffer,
        offset: 0,
        length: bytes.len(),
        kind: Kind::Uint8,
    };
    if let Some(destination) = window_mut(context, &view) {
        destination.copy_from_slice(bytes);
    }
    instance_over(context, view)
}

/// A `Buffer` instance over an EXISTING view — what [`slice`]/[`subarray`]
/// answer, sharing bytes rather than copying them.
fn instance_over(context: &mut Context, view: View) -> u64 {
    let Some(cell) = native::plain(context) else {
        return undefined_of(context);
    };
    super::register_buffer(context);
    if let Some(prototype) = super::super::class_support::prototype(context, "Buffer") {
        context.set_prototype(cell, prototype);
    }
    let parent = Value::from_slot(view.buffer).bits();
    super::super::buffers::attach(context, cell, view);
    // Node exposes the backing ArrayBuffer through the legacy `parent` alias;
    // keep it identical to `buffer` so zero-length and shared views retain the
    // same identity as the source storage.
    let parent_key = context.well_known("parent");
    super::super::objects::put(context, cell, parent_key, parent);
    Value::from_slot(cell).bits()
}

/// A Buffer view over an existing raw ArrayBuffer, preserving shared storage.
pub(in crate::entry) fn from_array_buffer(context: &mut Context, source: u64) -> u64 {
    let Some(buffer) = Value(source).as_slot() else {
        return undefined_of(context);
    };
    let Some(length) = context.bytes_at(buffer).map(Vec::len) else {
        return undefined_of(context);
    };
    instance_over(
        context,
        View {
            buffer,
            offset: 0,
            length,
            kind: Kind::Uint8,
        },
    )
}

/// The bytes a value carries as source data: a `Uint8Array`/`Buffer`'s window,
/// a string encoded per `encoding`, or a JS array of numbers.
pub(in crate::entry) fn source_bytes(context: &Context, value: u64, encoding: &str) -> Option<Vec<u8>> {
    if let Some(view) = view_of(context, value) {
        return window(context, &view).map(<[u8]>::to_vec);
    }
    if let Some(cell) = Value(value).as_slot()
        && let Some(text) = context.text_at(cell)
    {
        return codec::encode(&text.to_rust()?, encoding);
    }
    if let Some(cell) = Value(value).as_slot()
        && let Some(elements) = context.elements_at(cell)
    {
        return Some(
            elements
                .iter()
                .map(|element| super::super::operators::as_number(context, Value(*element)).unwrap_or(0.0) as u8)
                .collect(),
        );
    }
    None
}

/// An `encoding` argument, defaulting to `"utf8"` for `undefined`.
fn encoding_arg(context: &Context, value: u64) -> String {
    if value == undefined_of(context) {
        return "utf8".to_owned();
    }
    super::super::text::to_text(context, Value(value))
        .and_then(|text| text.to_rust())
        .unwrap_or_else(|| "utf8".to_owned())
}

/// A byte, a string's encoded bytes, or a view's bytes — what
/// [`fill`]/[`index_of`]/[`includes`] search for or write.
pub(in crate::entry) fn pattern_of(context: &Context, value: u64, encoding: &str) -> Vec<u8> {
    match Value(value).as_slot() {
        Some(cell) => {
            if let Some(text) = context.text_at(cell) {
                return codec::encode(&text.to_rust().unwrap_or_default(), encoding).unwrap_or_default();
            }
            if let Some(view) = view_of(context, value) {
                return window(context, &view).map(<[u8]>::to_vec).unwrap_or_default();
            }
            Vec::new()
        }
        None => {
            let number = super::super::operators::as_number(context, Value(value)).unwrap_or(0.0);
            // ToUint8 is modulo 256, while Rust's float-to-u8 cast saturates
            // negative values to zero. Buffer searches/fills rely on the JS
            // conversion, e.g. -140 becomes 116 (`'t'`).
            let byte = if number.is_finite() {
                number.trunc().rem_euclid(256.0) as u8
            } else {
                0
            };
            vec![byte]
        },
    }
}

// ---------------------------------------------------------------------------
// Instance methods
// ---------------------------------------------------------------------------

/// `buf.write(string, offset?, length?, encoding?)`.
///
/// # The two-argument form, and why a string `offset` is not always one
///
/// `buf.write(string, encoding)` is legal — Node reads a string second argument
/// as the encoding. What it does NOT allow is that shorthand with anything after
/// it: `buf.write('o', '1', 'ascii')` and `buf.write('test', 'utf8', 0)` are both
/// `ERR_INVALID_ARG_TYPE`, because the caller has now given an encoding twice or
/// a length after an offset that is not one. Both are in `test-buffer-alloc.js`,
/// and both used to be accepted here — the string coerced to an offset of 0 and
/// the write silently landed at the start of the buffer.
pub(in crate::entry) fn write(this: u64, string: u64, offset: u64, length: u64, encoding: u64) -> f64 {
    let Some(count) = validate::bytes("source", this) else { return 0.0 };
    let Shape::Text(text) = validate::shape_of(string) else {
        errors::invalid_arg_type("string", "string", string);
        return 0.0;
    };
    let shorthand = matches!(validate::shape_of(offset), Shape::Text(_));
    let (offset, encoding) = match shorthand {
        true => {
            let trailing = !matches!(validate::shape_of(length), Shape::Absent)
                || !matches!(validate::shape_of(encoding), Shape::Absent);
            if trailing {
                errors::invalid_arg_type("offset", "number", offset);
                return 0.0;
            }
            (undefined(), offset)
        }
        false => (offset, encoding),
    };
    let Some(enc) = validate::encoding(encoding) else { return 0.0 };
    let Some(start) = validate::offset("offset", offset, count) else { return 0.0 };
    let cap = match validate::shape_of(length) {
        Shape::Absent => count - start,
        _ => match validate::offset("length", length, count - start) {
            Some(length) => length,
            None => return 0.0,
        },
    };
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return 0.0 };
        let bytes = codec::encode(&text, &enc).unwrap_or_default();
        let count = bytes.len().min(cap).min(view.count() - start);
        if let Some(destination) = window_mut(context, &view) {
            destination[start..start + count].copy_from_slice(&bytes[..count]);
        }
        count as f64
    })
}

/// `buf.slice(begin?, end?)` / `buf.subarray(begin?, end?)` — a Buffer
/// SHARING the same bytes, which is Node's own divergence from
/// `TypedArray.prototype.slice`'s copy (see the module doc on `mod.rs`).
pub(in crate::entry) fn windowed(this: u64, begin: u64, end: u64) -> u64 {
    let begin = optional_number(begin);
    let end = optional_number(end);
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = view_of(context, this) else { return absent };
        let (first, last) = range(view.count(), begin, end);
        let size = view.kind.size();
        instance_over(
            context,
            View {
                buffer: view.buffer,
                offset: view.offset + first * size,
                length: (last - first) * size,
                kind: view.kind,
            },
        )
    })
}

/// `buf.equals(other)`.
///
/// `"otherBuffer"` is what Node calls the argument here and not `"target"` —
/// the same object in a different member's documentation, and the tests compare
/// the sentence.
/// The refusal is `compare`'s and is not repeated here: a second
/// [`validate::bytes`] would classify the same two values again, and asking
/// [`super::super::throw::in_flight`] afterwards is the rule this crate's README
/// states for exactly this — a native that called something which may have
/// raised checks before it believes the answer. `0.0` means *equal* and is also
/// what a refused compare answers, so believing it would make
/// `buf.equals('abc')` report `true` on its way out.
pub(in crate::entry) fn equals(this: u64, other: u64) -> bool {
    let ordering = super::statics::compare_values(this, other, "source", "otherBuffer");
    !super::super::throw::in_flight() && ordering == 0.0
}

/// `buf.copy(target, targetStart?, sourceStart?, sourceEnd?)`.
///
/// The three bounds go through [`validate::offset`] rather than [`range`]'s
/// clamping, and that is the difference `test-buffer-copy.js` asserts: a
/// NEGATIVE `targetStart` is `ERR_OUT_OF_RANGE` in Node, where `range` would
/// read it as counting from the end and copy somewhere the caller did not ask
/// for. `<=` the length rather than `<`: an empty copy at the very end is legal.
pub(in crate::entry) fn copy(this: u64, target: u64, target_start: u64, source_start: u64, source_end: u64) -> f64 {
    // One check at a time, and each returns: two raised in the same expression
    // would leave the SECOND in flight, and the slot holds one throw — a program
    // catching `copy(target, -1, -1)` would be told about the argument it did
    // not ask about first.
    let Some(source_len) = validate::bytes("source", this) else { return 0.0 };
    let Some(target_len) = validate::bytes("target", target) else { return 0.0 };
    let Some(target_at) = validate::offset("targetStart", target_start, target_len) else {
        return 0.0;
    };
    let Some(source_at) = validate::offset("sourceStart", source_start, source_len) else {
        return 0.0;
    };
    let source_to = match validate::shape_of(source_end) {
        Shape::Absent => source_len,
        _ => match validate::offset("sourceEnd", source_end, source_len) {
            Some(at) => at,
            None => return 0.0,
        },
    };
    let target_start = Some(target_at as f64);
    let source_start = Some(source_at as f64);
    let source_end = Some(source_to as f64);
    with_current(|context| {
        let Some(source) = view_of(context, this) else { return 0.0 };
        let Some(destination) = view_of(context, target) else { return 0.0 };
        let (first, last) = range(source.count(), source_start, source_end);
        let Some(bytes) = window(context, &source) else { return 0.0 };
        let taken: Vec<u8> = bytes[first..last].to_vec();
        let start = as_count(target_start.unwrap_or(0.0)).min(destination.count());
        let count = taken.len().min(destination.count() - start);
        if let Some(into) = window_mut(context, &destination) {
            into[start..start + count].copy_from_slice(&taken[..count]);
        }
        count as f64
    })
}

/// `buf.fill(value, begin?, end?, encoding?)`.
pub(in crate::entry) fn fill(this: u64, value: u64, begin: u64, end: u64, encoding: u64) -> u64 {
    let begin = optional_number(begin);
    let end = optional_number(end);
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return this };
        let (first, last) = range(view.count(), begin, end);
        let enc = encoding_arg(context, encoding);
        let pattern = pattern_of(context, value, &enc);
        if !pattern.is_empty()
            && let Some(bytes) = window_mut(context, &view)
        {
            for (offset, at) in (first..last).enumerate() {
                bytes[at] = pattern[offset % pattern.len()];
            }
        }
        this
    })
}

/// The first index at or after `from` where `needle` occurs in `haystack`, or
/// `None`. An empty needle matches at `from` itself, the way `String.indexOf`
/// treats one.
fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return (from <= haystack.len()).then_some(from);
    }
    if from >= haystack.len() {
        return None;
    }
    // Two-way with a SIMD prefilter, where the window compare this replaces was
    // O(n*m). The same swap `entry::string::basic` took, and this one is the
    // case that wants it most: a `Buffer` is bytes by construction and is
    // routinely megabytes, where a string in this engine is usually short.
    memchr::memmem::find(&haystack[from..], needle).map(|position| position + from)
}

/// `buf.indexOf(value, byteOffset?, encoding?)`.
pub(in crate::entry) fn index_of(this: u64, value: u64, byte_offset: u64, encoding: u64) -> f64 {
    if !validate::search_value(value) {
        return -1.0;
    }
    let byte_offset = optional_number(byte_offset);
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return -1.0 };
        let Some(bytes) = window(context, &view) else { return -1.0 };
        let enc = encoding_arg(context, encoding);
        let needle = pattern_of(context, value, &enc);
        // UTF-16 searches operate on two-byte code units. A raw Buffer needle
        // with an odd byte length cannot represent one, and Node returns no
        // match rather than finding a byte halfway through a code unit.
        if entry::canonical_encoding(&enc) == Some("utf16le") && needle.len() % 2 != 0 {
            return -1.0;
        }
        let (from, _) = range(bytes.len(), byte_offset, None);
        find(bytes, &needle, from).map(|at| at as f64).unwrap_or(-1.0)
    })
}

/// `buf.includes(value, byteOffset?, encoding?)`.
pub(in crate::entry) fn includes(this: u64, value: u64, byte_offset: u64, encoding: u64) -> bool {
    index_of(this, value, byte_offset, encoding) >= 0.0
}

/// `buf.toJSON()` — `{ type: "Buffer", data: [...] }`.
pub(in crate::entry) fn to_json(this: u64) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = view_of(context, this) else { return absent };
        let bytes = window(context, &view).map(<[u8]>::to_vec).unwrap_or_default();
        let Some(cell) = native::plain(context) else { return absent };
        let type_value = context.intern_value(crate::text::Str::from_str("Buffer")).bits();
        let type_key = context.well_known("type");
        super::super::objects::put(context, cell, type_key, type_value);
        let data = bytes.iter().map(|byte| Value::from_f64(f64::from(*byte)).bits()).collect();
        let data_value = super::super::array::built_in(context, data);
        let data_key = context.well_known("data");
        super::super::objects::put(context, cell, data_key, data_value);
        Value::from_slot(cell).bits()
    })
}

// ---------------------------------------------------------------------------
// The numeric family — every read and write goes through `buffers::element`,
// the same codec the typed arrays use. See that module's doc for why writing
// wraps and why the endianness gathering is written once.
// ---------------------------------------------------------------------------

/// One element, read at a byte offset.
///
/// An offset that is not a number, not an integer, or does not leave room for
/// the element is refused rather than answered. It used to answer `NaN`, which
/// is the wrong answer twice over: `NaN` is also what a legitimate read of a
/// float can produce, so a program could not tell a bad offset from bad data.
pub(in crate::entry) fn read_num(this: u64, offset: u64, kind: Kind, little: bool) -> f64 {
    let Some(count) = validate::bytes("buffer", this) else { return f64::NAN };
    let Some(at) = validate::element_offset("offset", offset, count, kind.size()) else {
        return f64::NAN;
    };
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return f64::NAN };
        let Some(bytes) = window(context, &view) else { return f64::NAN };
        super::super::buffers::element::read(bytes, at, kind, little).unwrap_or(f64::NAN)
    })
}

/// One element, written at a byte offset. Answers `offset + the element's
/// width`, which is what Node's writes answer.
///
/// The VALUE is range-checked as well as the offset, and that is not the same
/// question: `buf.writeUInt8(256)` fits the buffer perfectly and is still
/// refused, because the byte stored would be `0` — the codec wraps by design
/// (see `buffers::element`) and a silent wrap is what
/// `test-buffer-writeuint.js` is written to catch.
pub(in crate::entry) fn write_num(this: u64, value: f64, offset: u64, kind: Kind, little: bool) -> f64 {
    let Some(count) = validate::bytes("buffer", this) else { return 0.0 };
    let Some(at) = validate::element_offset("offset", offset, count, kind.size()) else {
        return 0.0;
    };
    if !validate::fits(kind, value) {
        return 0.0;
    }
    with_current(|context| {
        if let Some(view) = view_of(context, this)
            && let Some(bytes) = window_mut(context, &view)
        {
            super::super::buffers::element::write(bytes, at, kind, value, little);
        }
        (at + kind.size()) as f64
    })
}

/// A variable-width integer read, used by the 1–6 byte Buffer methods.
pub(in crate::entry) fn read_variable(
    this: u64,
    offset: u64,
    byte_length: u64,
    little: bool,
    signed: bool,
) -> f64 {
    let Some(count) = validate::bytes("buffer", this) else { return f64::NAN };
    let Some(width) = validate::byte_length(byte_length) else { return f64::NAN };
    let Some(at) = validate::variable_offset(offset, count, width) else { return f64::NAN };
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return f64::NAN };
        let Some(bytes) = window(context, &view) else { return f64::NAN };
        let Some(word) = super::super::buffers::element::gathered(bytes, at, width, little) else {
            return f64::NAN;
        };
        let bits = width * 8;
        if signed && (word & (1u64 << (bits - 1))) != 0 {
            (word as i128 - (1i128 << bits)) as f64
        } else {
            word as f64
        }
    })
}

/// A variable-width integer write, used by the 1–6 byte Buffer methods.
pub(in crate::entry) fn write_variable(
    this: u64,
    value: f64,
    offset: u64,
    byte_length: u64,
    little: bool,
    signed: bool,
) -> f64 {
    let Some(count) = validate::bytes("buffer", this) else { return 0.0 };
    let Some(width) = validate::byte_length(byte_length) else { return 0.0 };
    let Some(at) = validate::variable_offset(offset, count, width) else { return 0.0 };
    if !validate::variable_fits(value, width, signed) {
        return 0.0;
    }
    let bits = width * 8;
    let word = (value as i128).rem_euclid(1i128 << bits) as u64;
    with_current(|context| {
        if let Some(view) = view_of(context, this)
            && let Some(bytes) = window_mut(context, &view)
        {
            for index in 0..width {
                let shift = if little { index } else { width - 1 - index };
                bytes[at + index] = (word >> (shift * 8)) as u8;
            }
        }
        (at + width) as f64
    })
}
