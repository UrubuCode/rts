//! `buf.fill(value, offset?, end?, encoding?)` — Node's `_fill`, argument for
//! argument.
//!
//! Left [`super::ops`] for the same reason the statics did: the crate's 500-line
//! ceiling, and because the argument juggling is the whole difficulty. Node's
//! rules, which this follows in Node's order:
//!
//! - For a STRING `value`, an `offset` that is absent or a string is really the
//!   encoding (`fill('ab', 'hex')`), and the fill covers the whole buffer; a
//!   string `end` is the encoding (`fill('ab', 1, 'hex')`). Only when `offset`
//!   is a number does the fourth argument count. For any other `value` the
//!   encoding is never consulted (`fill(1, 0, 2, 'nope')` is fine).
//! - The encoding is validated before the range, the range before the value:
//!   an unknown encoding is `ERR_UNKNOWN_ENCODING`, a non-number bound
//!   `ERR_INVALID_ARG_TYPE`, a non-integer one `ERR_OUT_OF_RANGE (an integer)`,
//!   an `end` outside `0..=length` (or an `offset` below zero)
//!   `ERR_OUT_OF_RANGE`; an `offset` at or past `end` is a no-op.
//! - An empty string fills with zero. A non-empty string that encodes to
//!   nothing (`'zz'` as hex) or an empty view is `ERR_INVALID_ARG_VALUE`.
//!
//! The ABI hands a missing trailing argument as `undefined` or `null`; both mean
//! "not given" here, which makes an explicit `fill(1, null)` quietly work where
//! Node throws. Telling them apart needs the argument count, which the native
//! signature does not carry.

use super::super::buffers::{view_of, window_mut};
use super::super::errors;
use super::super::objects::undefined_of;
use super::super::with_current;
use super::ops::pattern_of;
use super::validate::{self, Shape};
use crate::entry;

/// The largest offset Node accepts, `Number.MAX_SAFE_INTEGER`.
const MAX_SAFE: f64 = 9007199254740991.0;

fn is_absent(value: u64) -> bool {
    value == entry::undefined_value() || value == entry::null_value()
}

fn is_text(value: u64) -> bool {
    matches!(validate::shape_of(value), Shape::Text(_))
}

/// A bound argument as an integer, or `None` with a throw in flight.
fn bound(name: &str, value: u64, max: f64) -> Option<f64> {
    let number = match validate::shape_of(value) {
        Shape::Number(number) => number,
        _ => {
            errors::invalid_arg_type(name, "number", value);
            return None;
        }
    };
    if !number.is_finite() || number.fract() != 0.0 {
        errors::out_of_range(name, "an integer", value);
        return None;
    }
    if number < 0.0 || number > max {
        errors::out_of_range(name, &format!(">= 0 && <= {max}"), value);
        return None;
    }
    Some(number)
}

pub(in crate::entry) fn fill(this: u64, value: u64, begin: u64, end: u64, encoding: u64) -> u64 {
    let Some(length) = with_current(|context| view_of(context, this).map(|view| view.count())) else {
        return this;
    };
    let text_value = is_text(value);
    let (mut begin, mut end, mut encoding) = (begin, end, encoding);
    if text_value {
        if is_absent(begin) || is_text(begin) {
            encoding = begin;
            begin = with_current(|context| undefined_of(context));
            end = begin;
        } else if is_text(end) {
            encoding = end;
            end = with_current(|context| undefined_of(context));
        }
    }
    let name = if text_value && !is_absent(encoding) {
        match validate::shape_of(encoding) {
            Shape::Text(text) if text.is_empty() => Some(String::from("utf8")),
            _ => validate::encoding(encoding),
        }
    } else {
        Some(String::from("utf8"))
    };
    let Some(name) = name else { return this };

    let first = if is_absent(begin) {
        0.0
    } else {
        let Some(first) = bound("offset", begin, MAX_SAFE) else { return this };
        first
    };
    let last = if is_absent(end) {
        length as f64
    } else {
        let Some(last) = bound("end", end, length as f64) else { return this };
        last
    };
    if first >= last {
        return this;
    }

    let shape = validate::shape_of(value);
    let pattern = with_current(|context| match &shape {
        Shape::Text(text) if text.is_empty() => vec![0u8],
        Shape::Text(_) | Shape::Bytes(_) => pattern_of(context, value, &name),
        Shape::Number(_) | Shape::Absent => pattern_of(context, value, &name),
        _ => {
            let held = pattern_of(context, value, &name);
            if held.is_empty() { vec![0u8] } else { held }
        }
    });
    if pattern.is_empty() {
        errors::invalid_arg_value("value", value, "is invalid");
        return this;
    }
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return };
        if let Some(bytes) = window_mut(context, &view) {
            for (offset, at) in (first as usize..last as usize).enumerate() {
                bytes[at] = pattern[offset % pattern.len()];
            }
        }
    });
    this
}
