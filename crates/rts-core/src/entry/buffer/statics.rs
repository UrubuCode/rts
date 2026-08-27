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
pub(in crate::entry) fn from(source: u64, encoding_or_offset: u64) -> u64 {
    let Some(shape) = validate::source("value", source) else {
        return undefined();
    };
    if matches!(shape, Shape::ArrayBuffer) {
        return with_current(|context| super::ops::from_array_buffer(context, source));
    }
    let encoding = match shape {
        Shape::Text(_) => match validate::encoding(encoding_or_offset) {
            Some(name) => name,
            None => return undefined(),
        },
        _ => String::from("utf8"),
    };
    with_current(|context| {
        let bytes = source_bytes(context, source, &encoding).unwrap_or_default();
        made(context, &bytes)
    })
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
    let Some(shape) = validate::source("string", source) else {
        return 0.0;
    };
    let encoding = match shape {
        Shape::Text(_) => match validate::encoding(encoding) {
            Some(name) => name,
            None => return 0.0,
        },
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

