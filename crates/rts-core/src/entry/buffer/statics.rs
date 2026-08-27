//! The `Buffer.*` STATICS — `alloc`, `from`, `concat`, `byteLength`, the two
//! predicates and the static `compare`.
//!
//! # Why they left [`super::ops`]
//!
//! Size, and a line the statics do not share with the instance methods. Every
//! member here validates arguments the caller INVENTED — a byte count, a list,
//! an encoding name — where an instance method starts from a buffer that already
//! exists and asks where inside it something goes. Adding Node's refusals took
//! `ops.rs` past the crate's 500-line ceiling, and this is the seam it was
//! already divided along: the file had `// Statics` and `// Instance methods`
//! banners over exactly these two halves.
//!
//! The shared helpers stayed in [`super::ops`] rather than moving with the
//! statics — `made`, `source_bytes`, `pattern_of` are read by both halves, and a
//! copy on each side is the duplication this crate's rule 3 is about.
//!
//! The borrow rule is unchanged and is [`super::ops`]'s: every member takes ONE,
//! at the end, after every refusal has been decided. See [`super::validate`].

use super::super::array_proto::arguments_at;
use super::super::buffers::{undefined, view_of, window};
use super::super::errors;
use super::super::objects::undefined_of;
use super::super::{operators, with_current};
use crate::coerce::Hint;
use crate::value::Value;
use super::codec;
use super::ops::{made, pattern_of, source_bytes};
use super::validate::{self, Shape};


/// `Buffer.alloc(size, fill?, encoding?)`.
///
/// The three refusals are Node's, in Node's order, and every one of them was a
/// wrong answer here before: a non-number size clamped to zero through
/// `as_count`, a negative one did too, and a `fill` that encodes to nothing
/// produced a zero-filled buffer that looks exactly like a successful call.
/// `.length` silently reading 0 instead of the error a caller is checking for is
/// the wrong-answer-over-throw case rule 8's neighbourhood exists to avoid.
pub(in crate::entry) fn alloc(size: u64, fill: u64, encoding: u64) -> u64 {
    let Some(length) = validate::size("size", size) else {
        return undefined();
    };
    let Some(enc) = validate::encoding(encoding) else {
        return undefined();
    };
    // Classified before the borrow, and asked about before the fill is built:
    // an empty pattern is indistinguishable from "no fill asked for" once
    // `repeated` has turned it into zeroes.
    if empty_fill(fill, &enc) {
        errors::invalid_arg_value("value", fill, "is invalid");
        return undefined();
    }
    with_current(|context| {
        let absent = undefined_of(context);
        let pattern = if fill == absent {
            Vec::new()
        } else {
            pattern_of(context, fill, &enc)
        };
        let bytes = repeated(&pattern, length);
        made(context, &bytes)
    })
}

/// Whether a `fill` was given and carries no bytes — `Buffer.alloc(4, 'zz',
/// 'hex')` and `Buffer.alloc(1, Buffer.alloc(0))`, both `ERR_INVALID_ARG_VALUE`
/// in Node. A `fill` that is genuinely absent is not one of these.
fn empty_fill(fill: u64, encoding: &str) -> bool {
    match validate::shape_of(fill) {
        Shape::Text(text) => {
            !text.is_empty() && codec::encode(&text, encoding).is_none_or(|bytes| bytes.is_empty())
        }
        Shape::Bytes(count) => count == 0,
        _ => false,
    }
}

/// `Buffer.allocUnsafe(size)` — zero-filled here (see the module doc).
///
/// Same size refusal as [`alloc`], for the same reason.
pub(in crate::entry) fn alloc_unsafe(size: u64) -> u64 {
    let Some(length) = validate::size("size", size) else {
        return undefined();
    };
    with_current(|context| made(context, &vec![0u8; length]))
}

/// `Buffer.from(source, encodingOrOffset?)`.
///
/// The second argument is an ENCODING only when the source is a string — for a
/// view or an array it is a byte offset — so it is validated as one only there.
/// Checking it always would refuse `Buffer.from(other, 2)`, which is legal.
pub(in crate::entry) fn from(
    source: u64,
    encoding_or_offset: u64,
    length: u64,
) -> u64 {
    let source = string_wrapper_source(source);
    let source = primitive_source(source);
    if super::super::throw::in_flight() {
        return undefined();
    }
    let Some(shape) = validate::source("value", source) else {
        return undefined();
    };
    if matches!(shape, Shape::ArrayBuffer) {
        let Some((offset, length)) = array_buffer_range(source, encoding_or_offset, length) else {
            return undefined();
        };
        return with_current(|context| {
            super::ops::from_array_buffer(context, source, offset, length)
        });
    }
    let encoding = match shape {
        Shape::Text(_) => match validate::shape_of(encoding_or_offset) {
            Shape::Text(_) => match validate::encoding(encoding_or_offset) {
                Some(name) => name,
                None => return undefined(),
            },
            _ => String::from("utf8"),
        },
        _ => String::from("utf8"),
    };
    let bytes = match shape {
        Shape::Other => match with_current(|context| object_source_bytes(context, source)) {
            Some(bytes) => bytes,
            None => {
                errors::invalid_buffer_source(source);
                return undefined();
            }
        },
        _ => with_current(|context| source_bytes(context, source, &encoding)).unwrap_or_default(),
    };
    with_current(|context| made(context, &bytes))
}

/// Unwraps only String wrapper objects. Number and Boolean wrappers must remain
/// objects so their refusal reports `an instance of Number`/`Boolean`.
fn string_wrapper_source(value: u64) -> u64 {
    with_current(|context| {
        let primitive = super::super::primitive_proto::unwrap(context, value);
        let Some(cell) = Value(primitive).as_slot() else {
            return value;
        };
        context.text_at(cell).is_some().then_some(primitive).unwrap_or(value)
    })
}

/// Invokes `Symbol.toPrimitive` only when the source actually exposes that hook.
/// Generic objects are not coerced: Node rejects `{}` even though ordinary
/// JavaScript numeric/string conversion could produce a primitive for it.
fn primitive_source(value: u64) -> u64 {
    if !with_current(|context| super::super::primitive::is_object_in(context, value)) {
        return value;
    }
    let key = with_current(|context| super::super::symbol::well_known(context, "toPrimitive"));
    let method = super::super::computed::get_indexed(value, key);
    if super::super::throw::in_flight() {
        return value;
    }
    if !with_current(|context| super::super::modules::is_callable_in(context, method)) {
        return value;
    }
    let (hint, absent) = with_current(|context| {
        (
            context.intern_value(crate::text::Str::from_str("string")).bits(),
            undefined_of(context),
        )
    });
    let answer = super::super::functions::call(method, value, hint, absent, absent, absent);
    if super::super::throw::in_flight() {
        value
    } else {
        answer
    }
}

/// Reads the object forms Node accepts as a Buffer source.
fn object_source_bytes(context: &mut super::super::Context, value: u64) -> Option<Vec<u8>> {
    let cell = Value(value).as_slot()?;
    let type_key = context.well_known("type");
    if let Some(kind) = super::super::objects::read_property(context, cell, type_key)
        .and_then(|kind| kind.as_slot())
        .and_then(|kind| context.text_at(kind))
        .and_then(|kind| kind.to_rust())
        && kind == "Buffer"
    {
        let data_key = context.well_known("data");
        let data = super::super::objects::read_property(context, cell, data_key)?;
        return array_values(context, data.bits());
    }

    // An object carrying `buffer` is accepted as an Array-like source even when
    // it has no length; the resulting Buffer is empty, matching Node's legacy
    // structural overload and the SharedArrayBuffer regression test.
    let buffer_key = context.well_known("buffer");
    if super::super::objects::read_property(context, cell, buffer_key).is_some() {
        return Some(Vec::new());
    }

    let length_key = context.well_known("length");
    let length = super::super::objects::read_property(context, cell, length_key)?;
    let number = operators::as_number(context, length).unwrap_or(f64::NAN);
    let count = super::super::buffers::as_count(number);
    let mut bytes = Vec::with_capacity(count);
    for index in 0..count {
        let key = crate::object::Key::Index(index as u32);
        let held = super::super::objects::read_property(context, cell, key)
            .map(|value| operators::as_number(context, value).unwrap_or(0.0))
            .unwrap_or(0.0);
        bytes.push(if held.is_finite() {
            held.trunc().rem_euclid(256.0) as u8
        } else {
            0
        });
    }
    Some(bytes)
}

/// Copies numeric entries from a JSON Buffer `data` array.
fn array_values(context: &super::super::Context, value: u64) -> Option<Vec<u8>> {
    let cell = Value(value).as_slot()?;
    let elements = context.elements_at(cell)?;
    Some(
        elements
            .iter()
            .map(|value| operators::as_number(context, Value(*value)).unwrap_or(0.0))
            .map(|number| {
                if number.is_finite() {
                    number.trunc().rem_euclid(256.0) as u8
                } else {
                    0
                }
            })
            .collect(),
    )
}

/// Resolves `Buffer.from(arrayBuffer, byteOffset?, length?)` before the view
/// borrow opens. Non-numeric offsets become zero, a missing length covers the
/// remainder, and a finite range that leaves the backing store becomes the
/// Node-specific buffer-bounds error.
fn array_buffer_range(source: u64, offset: u64, length: u64) -> Option<(usize, usize)> {
    let total = with_current(|context| {
        Value(source)
            .as_slot()
            .and_then(|cell| context.bytes_at(cell).map(Vec::len))
    })?;
    let offset = super::super::buffers::optional_number(offset).unwrap_or(0.0);
    let offset = if offset.is_nan() {
        0
    } else if offset.is_finite() && offset.fract() == 0.0 && offset >= 0.0 {
        offset as usize
    } else {
        errors::buffer_out_of_bounds(Some("offset"));
        return None;
    };
    if offset > total {
        errors::buffer_out_of_bounds(Some("offset"));
        return None;
    }
    let Some(length) = super::super::buffers::optional_number(length) else {
        return Some((offset, total - offset));
    };
    if length.is_nan() {
        return Some((offset, 0));
    }
    if !length.is_finite() || length.fract() != 0.0 || length < 0.0 {
        errors::buffer_out_of_bounds(Some("length"));
        return None;
    }
    let length = length as usize;
    if length > total - offset {
        errors::buffer_out_of_bounds(Some("length"));
        return None;
    }
    Some((offset, length))
}

/// `Buffer.of(...values)`.
///
/// This is deliberately not routed through [`source_bytes`]: each argument is
/// one numeric ToUint8 value, so `Buffer.of("2")` contains `2` rather than the
/// UTF-8 byte for the character `2`.
pub(in crate::entry) fn of(a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let values = with_current(|context| arguments_at(context, 0, [a0, a1, a2, a3]));
    let mut bytes = Vec::with_capacity(values.len());
    for value in values {
        let primitive = super::super::primitive::to_primitive(value, Hint::Number);
        if super::super::throw::in_flight() {
            return undefined();
        }
        let number = with_current(|context| {
            operators::as_number(context, Value(primitive)).unwrap_or(0.0)
        });
        bytes.push(if number.is_finite() {
            number.trunc().rem_euclid(256.0) as u8
        } else {
            0
        });
    }
    with_current(|context| made(context, &bytes))
}

/// `Buffer.concat(list, totalLength?)`.
pub(in crate::entry) fn concat(list_value: u64, total_length: u64) -> u64 {
    let Some(elements) = validate::list(list_value) else {
        return undefined();
    };
    // Through `size` rather than `optional_number`: `Buffer.concat(list, -1)` is
    // `ERR_OUT_OF_RANGE` in Node and was a zero-length buffer here.
    let wanted = match validate::shape_of(total_length) {
        Shape::Absent => None,
        _ => match validate::size("totalLength", total_length) {
            Some(length) => Some(length),
            None => return undefined(),
        },
    };
    with_current(|context| {
        let mut joined = Vec::new();
        for element in elements {
            if let Some(bytes) = source_bytes(context, element, "utf8") {
                joined.extend_from_slice(&bytes);
            }
        }
        if let Some(wanted) = wanted {
            joined.resize(wanted, 0);
        }
        made(context, &joined)
    })
}

/// `Buffer.byteLength(source, encoding?)`.
///
/// `0.0` on refusal is not an answer: a throw is in flight by then and the
/// caller's check discards whatever crossed back. Every early return in this
/// module and in [`super::ops`] means the same thing, whatever value it names.
pub(in crate::entry) fn byte_length(source: u64, encoding: u64) -> f64 {
    let Some(shape) = validate::byte_length_source(source) else {
        return 0.0;
    };
    if matches!(shape, Shape::ArrayBuffer) {
        return with_current(|context| {
            Value(source)
                .as_slot()
                .and_then(|cell| context.bytes_at(cell))
                .map_or(0.0, |bytes| bytes.len() as f64)
        });
    }
    if matches!(shape, Shape::Bytes(_)) {
        return with_current(|context| {
            view_of(context, source)
                .map_or(0.0, |view| view.length as f64)
        });
    }
    let encoding = match validate::shape_of(encoding) {
        Shape::Text(name) => codec::canonical_encoding(&name)
            .unwrap_or("utf8")
            .to_owned(),
        _ => String::from("utf8"),
    };
    with_current(|context| {
        source_bytes(context, source, &encoding).map(|bytes| bytes.len() as f64).unwrap_or(0.0)
    })
}

/// `Buffer.isBuffer(value)` — whether it is a view at all (see the module
/// doc: nothing this runtime hands back that is not one).
pub(in crate::entry) fn is_buffer(value: u64) -> bool {
    with_current(|context| view_of(context, value).is_some())
}

/// `Buffer.isEncoding(name)`.
pub(in crate::entry) fn is_encoding(encoding: u64) -> bool {
    with_current(|context| {
        super::super::text::to_text(context, Value(encoding))
            .and_then(|text| text.to_rust())
            .is_some_and(|text| codec::canonical_encoding(&text).is_some())
    })
}

/// Bytewise comparison, shared by the static and the instance `compare`.
///
/// The argument NAMES differ between the two — Node says `"buf1"`/`"buf2"` for
/// `Buffer.compare(a, b)` and `"source"`/`"target"` for `a.compare(b)`, and
/// `test-buffer-compare.js` asserts both sentences — so they are passed in
/// rather than fixed here. Everything else about the refusal is identical, which
/// is why it is one function.
pub(in crate::entry) fn compare_values(a: u64, b: u64, first: &str, second: &str) -> f64 {
    if validate::bytes(first, a).is_none() || validate::bytes(second, b).is_none() {
        return 0.0;
    }
    with_current(|context| {
        let a = view_of(context, a).and_then(|view| window(context, &view)).map(<[u8]>::to_vec).unwrap_or_default();
        let b = view_of(context, b).and_then(|view| window(context, &view)).map(<[u8]>::to_vec).unwrap_or_default();
        match a.cmp(&b) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        }
    })
}

/// `pattern` repeated/truncated to exactly `length` bytes.
fn repeated(pattern: &[u8], length: usize) -> Vec<u8> {
    if pattern.is_empty() {
        return vec![0u8; length];
    }
    (0..length).map(|index| pattern[index % pattern.len()]).collect()
}

