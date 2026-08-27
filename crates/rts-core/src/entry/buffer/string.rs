//! The string-facing half of `Buffer`.
//!
//! `Buffer.toString` looks like another range operation, but its bounds are not
//! the bounds of `slice`: negative values clamp to zero instead of counting from
//! the end. Reusing [`super::super::buffers::range`] therefore makes a negative
//! start select the final bytes, which is a different API contract. The helper
//! below keeps that deliberate distinction local rather than weakening the
//! shared slice/subarray rule.
//!
//! Its encoding argument also has a separate contract. Node converts a supplied
//! value with the string hint before looking up a codec, so an object can provide
//! `toString()`, while `Buffer.from` and allocation APIs continue to use the
//! strict validator in [`super::validate`].

use super::super::buffers::{optional_number, undefined, view_of, window};
use super::super::errors;
use super::super::objects::undefined_of;
use super::super::with_current;
use super::codec;
use crate::coerce::Hint;
use crate::value::Value;

/// `buf.toString(encoding?, start?, end?)`.
pub(in crate::entry) fn to_string(this: u64, encoding: u64, start: u64, end: u64) -> u64 {
    let Some(enc) = encoding_arg(encoding) else {
        return undefined();
    };
    let start = optional_number(start);
    if super::super::throw::in_flight() {
        return undefined();
    }
    let end = optional_number(end);
    if super::super::throw::in_flight() {
        return undefined();
    }
    let Some((view, first, last)) = with_current(|context| {
        let view = view_of(context, this)?;
        let (first, last) = absolute_range(view.length, start, end);
        Some((view, first, last))
    }) else {
        return undefined();
    };
    if (last - first) as f64 > super::super::BUFFER_MAX_STRING_LENGTH {
        errors::string_too_long();
        return undefined();
    }
    with_current(|context| {
        let Some(bytes) = window(context, &view) else {
            return undefined_of(context);
        };
        let text = codec::decode(&bytes[first..last], &enc);
        context
            .intern_value(crate::text::Str::from_str(&text))
            .bits()
    })
}

/// A `toString` encoding, with Node's string-hint coercion and error split.
fn encoding_arg(value: u64) -> Option<String> {
    let raw = if value == undefined() {
        return Some("utf8".to_owned());
    } else {
        value
    };
    let primitive = super::super::primitive::to_primitive(raw, Hint::String);
    if super::super::throw::in_flight() {
        return None;
    }
    let name = with_current(|context| {
        super::super::text::to_text(context, Value(primitive)).and_then(|text| text.to_rust())
    });
    let Some(name) = name else {
        super::super::throw::type_error("Cannot convert a Symbol value to a string");
        return None;
    };
    match codec::canonical_encoding(&name) {
        Some(canonical) => Some(canonical.to_owned()),
        None => {
            errors::unknown_encoding(&name);
            None
        }
    }
}

/// Clamp `start` and `end` as absolute byte positions.
fn absolute_range(count: usize, start: Option<f64>, end: Option<f64>) -> (usize, usize) {
    let first = absolute_index(start.unwrap_or(0.0), count);
    let last = match end {
        Some(value) => absolute_index(value, count),
        None => count,
    };
    if last < first {
        (first, first)
    } else {
        (first, last)
    }
}

/// `ToIntegerOrInfinity`, followed by an absolute clamp into the view.
fn absolute_index(value: f64, count: usize) -> usize {
    if value.is_nan() || value == f64::NEG_INFINITY {
        return 0;
    }
    if value == f64::INFINITY {
        return count;
    }
    value.trunc().max(0.0).min(count as f64) as usize
}

#[cfg(test)]
mod tests {
    use super::absolute_range;

    #[test]
    fn to_string_bounds_are_absolute_and_empty_when_end_precedes_start() {
        assert_eq!(absolute_range(3, Some(-1.0), Some(3.0)), (0, 3));
        assert_eq!(absolute_range(3, Some(1.0), Some(-1.2)), (1, 1));
        assert_eq!(
            absolute_range(3, Some(f64::NAN), Some(f64::INFINITY)),
            (0, 3)
        );
        assert_eq!(absolute_range(3, Some(f64::INFINITY), None), (3, 3));
    }
}
