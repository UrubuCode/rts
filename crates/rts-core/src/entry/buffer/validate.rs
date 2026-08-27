//! What a `Buffer` member REFUSES, before it does anything.
//!
//! # Why this is a module and not a line at the top of each member
//!
//! Because the same three questions are asked by fourteen of them — is this a
//! number, is it inside the buffer, is this thing bytes at all — and Node
//! answers each with a specific `code` a test matches on. Written per member
//! that is fourteen chances to spell `ERR_OUT_OF_RANGE` as `ERR_INVALID_ARG_TYPE`
//! for an argument that is out of range, which is a failure that reads as
//! correct until a suite compares codes. `rts_core::entry::errors` owns the
//! raising; this owns the deciding, which is the split that module's doc asks
//! for ("what a given API accepts is that API's business").
//!
//! # The borrow rule, which is the reason for [`Shape`]
//!
//! `entry::errors` builds the message by reading the offending value, and it
//! takes a borrow of the context to do it. So does every classification here.
//! A validator that classified *inside* a borrow and raised there would take the
//! second borrow of a `RefCell` in an `extern "C"` frame that cannot unwind —
//! the process aborts rather than failing a test, which is the trap
//! `buffers::bounds` already documents for `optional_number`.
//!
//! [`Shape`] is what closes it: ONE borrow answers what the value is, the borrow
//! drops, and every refusal after that is taken on owned data. That is also why
//! it carries the string and the element list rather than a flag — a caller that
//! had to go back for the contents would take the second borrow anyway.
//!
//! # What is NOT decided here
//!
//! Clamping. `buffers::bounds::range` still resolves a `begin`/`end` pair the
//! language *defines* as clamping (`buf.slice(-3)`, `buf.toString('utf8', 0,
//! 99)`), and those are not errors in Node either. Only the arguments Node
//! actually refuses come through this module.

use super::super::buffers::element::Kind;
use super::super::buffers::view_of;
use super::super::objects::undefined_of;
use super::super::{errors, with_current};
use super::codec;
use crate::value::Value;

/// The largest `Buffer` this engine will allocate.
///
/// `i32::MAX`, which is `node:buffer`'s `kMaxLength` here — that module picks
/// the number and this enforces it. They are not two decisions: a size this
/// refuses is exactly the size `buffer.constants.MAX_LENGTH` told the program
/// about, and `test-buffer-over-max-length.js` reads the constant to build the
/// argument it expects to be refused.
pub const MAX_LENGTH: f64 = i32::MAX as f64;

/// What a value IS, read under a single borrow. See the module doc.
pub(in crate::entry) enum Shape {
    /// The argument was left off, or explicitly `undefined`.
    Absent,
    /// A real number — not a string that would coerce to one, which is the
    /// distinction every `ERR_INVALID_ARG_TYPE` in Node's buffer tests rests on
    /// (`buf.readUInt8('0')` throws where `buf.readUInt8(0)` does not).
    Number(f64),
    /// A string, and its contents.
    Text(String),
    /// An array, and its elements — unvalidated, which is `Buffer.concat`'s job.
    List(Vec<u64>),
    /// A `Buffer`, typed array or `DataView`, and how many bytes it is.
    Bytes(usize),
    /// A raw `ArrayBuffer`, whose storage can be exposed through a Buffer view.
    ArrayBuffer,
    /// Anything else: an object, a boolean, `null`, a function.
    Other,
}

/// Classifies a value in ONE borrow, so every refusal after it is safe.
pub(in crate::entry) fn shape_of(value: u64) -> Shape {
    with_current(|context| {
        if value == undefined_of(context) {
            return Shape::Absent;
        }
        // `numeric`, not a coercion: see [`Shape::Number`].
        if let Some(number) = Value(value).numeric() {
            return Shape::Number(number);
        }
        // Before the text test, because a `Buffer` is an object and this asks
        // what it IS rather than what it prints as.
        if let Some(view) = view_of(context, value) {
            return Shape::Bytes(view.count());
        }
        let Some(cell) = Value(value).as_slot() else {
            return Shape::Other;
        };
        if let Some(bytes) = context.bytes_at(cell) {
            let _ = bytes;
            return Shape::ArrayBuffer;
        }
        if let Some(text) = context.text_at(cell) {
            // Buffer's UTF-8 encoder replaces lone UTF-16 surrogates with
            // U+FFFD; refusing the whole source would make valid JavaScript
            // strings unusable as Buffer input.
            return Shape::Text(text.to_rust_lossy());
        }
        match context.elements_at(cell) {
            Some(elements) => Shape::List(elements.clone()),
            None => Shape::Other,
        }
    })
}

/// A byte count: `Buffer.alloc`'s and `allocUnsafe`'s `size`.
///
/// `None` means a throw is now in flight and the caller must return at once.
///
/// Node's three refusals, in its order — a non-number is a TYPE error, and
/// everything after that (negative, fractional, `NaN`, above the ceiling) is a
/// RANGE error. Getting that order wrong is what makes `Buffer.alloc('x')`
/// answer `ERR_OUT_OF_RANGE`, which reads as sensible and fails the suite.
pub(in crate::entry) fn size(name: &str, value: u64) -> Option<usize> {
    let number = match shape_of(value) {
        // `Buffer.alloc()` with nothing at all is Node's `ERR_INVALID_ARG_TYPE`
        // and not a zero-length buffer.
        Shape::Number(number) => number,
        _ => {
            errors::invalid_arg_type(name, "number", value);
            return None;
        }
    };
    if !number.is_finite() || number.fract() != 0.0 || number < 0.0 || number > MAX_LENGTH {
        errors::out_of_range(name, &format!(">= 0 and <= {MAX_LENGTH}"), value);
        return None;
    }
    Some(number as usize)
}

/// The numeric part of an offset, after type and integralness checks.
///
/// Keeping this first step separate matters for a short buffer: Node reports an
/// invalid type or a fractional offset before it reports that the element cannot
/// fit, but reports out-of-bounds for an integer, Infinity, or a negative value.
fn integer_offset(name: &str, value: u64) -> Option<f64> {
    let number = match shape_of(value) {
        Shape::Absent => 0.0,
        Shape::Number(number) => number,
        _ => {
            errors::invalid_arg_type(name, "number", value);
            return None;
        }
    };
    if number.is_nan() || (number.is_finite() && number.fract() != 0.0) {
        errors::out_of_range(name, "an integer", value);
        return None;
    }
    Some(number)
}

/// A `Buffer.write` offset or length, whose public message uses `&&`.
///
/// The generic offset validator cannot change spelling: `copy` and the legacy
/// numeric readers expose the older `and` wording, and their tests match it.
/// Keeping this one operation-specific avoids making a shared validator answer
/// two observable contracts.
pub(in crate::entry) fn write_offset(name: &str, value: u64, limit: usize) -> Option<usize> {
    let number = integer_offset(name, value)?;
    if !number.is_finite() || number < 0.0 || number > limit as f64 {
        errors::out_of_range(name, &format!(">= 0 && <= {limit}"), value);
        return None;
    }
    Some(number as usize)
}

/// A bound for the legacy `asciiWrite`/`latin1Write`/`utf8Write` methods.
///
/// These methods predate `Buffer.write` and report a bounds error for an
/// invalid offset rather than the newer numeric range diagnostic. Their length
/// still clamps at the remaining capacity, which is why the two cases stay
/// separate below.
pub(in crate::entry) fn legacy_offset(name: &str, value: u64, limit: usize) -> Option<usize> {
    let number = integer_offset(name, value)?;
    if !number.is_finite() || number < 0.0 || number > limit as f64 {
        errors::buffer_out_of_bounds(Some(name));
        return None;
    }
    Some(number as usize)
}

pub(in crate::entry) fn legacy_length(value: u64, limit: usize) -> Option<usize> {
    let number = integer_offset("length", value)?;
    if !number.is_finite() || number < 0.0 {
        errors::buffer_out_of_bounds(Some("length"));
        return None;
    }
    Some((number as usize).min(limit))
}

/// `Buffer.copy` uses JavaScript number coercion for its range arguments.
fn copy_integer(value: u64) -> Option<f64> {
    let number = super::super::class_support::to_number(value);
    if super::super::throw::in_flight() {
        return None;
    }
    Some(if number.is_nan() { 0.0 } else { number.trunc() })
}

pub(in crate::entry) fn copy_target(value: u64, limit: usize) -> Option<usize> {
    let number = copy_integer(value)?;
    if !number.is_finite() || number < 0.0 || number > limit as f64 {
        errors::out_of_range("targetStart", ">= 0", value);
        return None;
    }
    Some(number as usize)
}

pub(in crate::entry) fn copy_source_start(value: u64, limit: usize) -> Option<usize> {
    let number = copy_integer(value)?;
    if !number.is_finite() || number < 0.0 || number > limit as f64 {
        errors::out_of_range("sourceStart", &format!(">= 0 && <= {limit}"), value);
        return None;
    }
    Some(number as usize)
}

pub(in crate::entry) fn copy_source_end(value: u64, limit: usize) -> Option<usize> {
    let number = copy_integer(value)?;
    if !number.is_finite() || number < 0.0 {
        errors::out_of_range("sourceEnd", ">= 0", value);
        return None;
    }
    Some((number as usize).min(limit))
}

/// An argument that must be a `Buffer` or a `Uint8Array` — `compare`'s target,
/// `equals`'s other, `copy`'s destination, and the receiver of both.
///
/// Answers the byte COUNT rather than a boolean, because every caller needs it
/// next and asking again is a second borrow. `None` means a throw is in flight.
pub(in crate::entry) fn bytes(name: &str, value: u64) -> Option<usize> {
    match shape_of(value) {
        Shape::Bytes(count) => Some(count),
        _ => {
            errors::invalid_arg_instance(name, "Buffer or Uint8Array", value);
            None
        }
    }
}

/// A value accepted by `Buffer.indexOf`/`includes`: number, string or bytes.
pub(in crate::entry) fn search_value(value: u64) -> bool {
    match shape_of(value) {
        Shape::Number(_) | Shape::Text(_) | Shape::Bytes(_) => true,
        _ => {
            errors::invalid_search_value(value);
            false
        }
    }
}

/// The offset of an ELEMENT `width` bytes wide inside a buffer of `count`.
///
/// The two refusals differ, which is why this is not [`offset`] with the
/// subtraction done at the call site: an offset past the last place the element
/// fits is `ERR_OUT_OF_RANGE`, but a buffer too short to hold ONE is
/// `ERR_BUFFER_OUT_OF_BOUNDS` — there is no in-range offset to report, so
/// naming a range would be describing an empty one.
pub(in crate::entry) fn element_offset(
    name: &str,
    value: u64,
    count: usize,
    width: usize,
) -> Option<usize> {
    let number = integer_offset(name, value)?;
    if width > count {
        errors::buffer_out_of_bounds(None);
        return None;
    }
    let limit = count - width;
    if !number.is_finite() || number < 0.0 || number > limit as f64 {
        errors::out_of_range(name, &format!(">= 0 and <= {limit}"), value);
        return None;
    }
    Some(number as usize)
}

/// A `Buffer.from`/`Buffer.byteLength` source.
///
/// Answers the [`Shape`] so the caller does not classify twice — the second
/// classification is a second borrow, which is what the module doc forbids.
pub(in crate::entry) fn source(name: &str, value: u64) -> Option<Shape> {
    match shape_of(value) {
        found @ (Shape::Text(_) | Shape::List(_) | Shape::Bytes(_) | Shape::ArrayBuffer) => {
            Some(found)
        }
        _ => {
            errors::invalid_arg_type(
                name,
                "string or an instance of Buffer, TypedArray, DataView, or Array",
                value,
            );
            None
        }
    }
}

/// `Buffer.concat`'s `list`: an array, every element of which is bytes.
///
/// Node names the offending element in the message (`"list[0]"`), which is the
/// half that makes a failure in a hundred-element concat findable.
pub(in crate::entry) fn list(value: u64) -> Option<Vec<u64>> {
    let Shape::List(elements) = shape_of(value) else {
        errors::invalid_arg_type("list", "Array", value);
        return None;
    };
    for (at, element) in elements.iter().enumerate() {
        if let Shape::Bytes(_) = shape_of(*element) {
            continue;
        }
        errors::invalid_arg_instance(&format!("list[{at}]"), "Buffer or Uint8Array", *element);
        return None;
    }
    Some(elements)
}

/// An `encoding` argument, canonicalised.
///
/// Three answers in one, because Node gives the three different codes: absent is
/// `"utf8"`, a non-string is `ERR_INVALID_ARG_TYPE`, and a string naming no
/// codec is `ERR_UNKNOWN_ENCODING` — `Buffer.from('', 'buffer')` is the case
/// that separates the last two, and it is in `test-buffer-alloc.js` precisely
/// because it once crashed rather than threw.
pub(in crate::entry) fn encoding(value: u64) -> Option<String> {
    let name = match shape_of(value) {
        Shape::Absent => return Some(String::from("utf8")),
        Shape::Text(name) => name,
        _ => {
            errors::invalid_arg_type("encoding", "string", value);
            return None;
        }
    };
    match codec::canonical_encoding(&name) {
        Some(canonical) => Some(canonical.to_owned()),
        None => {
            errors::unknown_encoding(&name);
            None
        }
    }
}

/// A byte length for `read/write{Int,UInt}{LE,BE}`.
pub(in crate::entry) fn byte_length(value: u64) -> Option<usize> {
    let number = match shape_of(value) {
        Shape::Number(number) => number,
        _ => {
            errors::invalid_arg_type("byteLength", "number", value);
            return None;
        }
    };
    if number.is_nan() || (number.is_finite() && number.fract() != 0.0) {
        errors::out_of_range("byteLength", "an integer", value);
        return None;
    }
    if !number.is_finite() || number < 1.0 || number > 6.0 {
        errors::out_of_range("byteLength", ">= 1 and <= 6", value);
        return None;
    }
    Some(number as usize)
}

/// A required offset for a variable-width integer operation.
pub(in crate::entry) fn variable_offset(value: u64, count: usize, width: usize) -> Option<usize> {
    let number = match shape_of(value) {
        Shape::Number(number) => number,
        _ => {
            errors::invalid_arg_type("offset", "number", value);
            return None;
        }
    };
    if number.is_nan() || (number.is_finite() && number.fract() != 0.0) {
        errors::out_of_range("offset", "an integer", value);
        return None;
    }
    let limit = count.saturating_sub(width);
    if !number.is_finite() || number < 0.0 || number > limit as f64 {
        if width > count {
            errors::buffer_out_of_bounds(None);
        } else {
            errors::out_of_range("offset", &format!(">= 0 and <= {limit}"), value);
        }
        return None;
    }
    Some(number as usize)
}

/// Whether a variable-width integer value fits its signedness and byte width.
pub(in crate::entry) fn variable_fits(value: f64, width: usize, signed: bool) -> bool {
    let bits = (width * 8) as u32;
    let (low, high) = if signed {
        (
            -(2f64.powi(bits as i32 - 1)),
            2f64.powi(bits as i32 - 1) - 1.0,
        )
    } else {
        (0.0, 2f64.powi(bits as i32) - 1.0)
    };
    if value.is_finite() && value.fract() == 0.0 && value >= low && value <= high {
        return true;
    }
    let expected = if signed && width > 4 {
        format!(">= -(2 ** {}) and < 2 ** {}", bits - 1, bits - 1)
    } else if !signed && width > 4 {
        format!(">= 0 and < 2 ** {bits}")
    } else if signed {
        format!(">= {low} and <= {high}")
    } else {
        format!(">= 0 and <= {high}")
    };
    errors::out_of_range_number("value", &expected, value, width > 4);
    false
}

/// Whether a number fits the integer element it is about to be written as.
///
/// `false` means a throw is in flight. The bounds come from [`Kind::integer`]
/// rather than from a table here, which is the point: the widths are already
/// decided in `buffers::element` and a second set of them would disagree the
/// first time one of the two grew a kind. A float element has no such bound and
/// is never refused.
pub(in crate::entry) fn fits(kind: Kind, value: f64) -> bool {
    let Some((bits, signed)) = kind.integer() else {
        return true;
    };
    let (low, high) = match signed {
        true => (
            -(2f64.powi(bits as i32 - 1)),
            2f64.powi(bits as i32 - 1) - 1.0,
        ),
        false => (0.0, 2f64.powi(bits as i32) - 1.0),
    };
    if value >= low && value <= high {
        return true;
    }
    // The raw bits back, so the message renders the number the program passed
    // rather than a re-parse of it.
    errors::out_of_range(
        "value",
        &format!(">= {low} and <= {high}"),
        Value::from_f64(value).bits(),
    );
    false
}
