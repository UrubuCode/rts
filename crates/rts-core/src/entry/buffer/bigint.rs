//! BigInt64 and BigUint64 accessors for Buffer views.

use super::super::buffers::element::{self, Kind};
use super::super::buffers::{view_of, window, window_mut};
use super::super::errors;
use super::super::objects::undefined_of;
use super::super::with_current;
use super::validate;

/// Read one 64-bit BigInt word from the current Buffer view.
pub(in crate::entry) fn read(this: u64, offset: u64, kind: Kind, little: bool) -> u64 {
    let Some(count) = validate::bytes("buffer", this) else {
        return super::super::buffers::undefined();
    };
    let Some(at) = validate::element_offset("offset", offset, count, kind.size()) else {
        return super::super::buffers::undefined();
    };
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = view_of(context, this) else {
            return absent;
        };
        let Some(bytes) = window(context, &view) else {
            return absent;
        };
        let Some(word) = element::word_at(bytes, at, kind, little) else {
            return absent;
        };
        super::super::buffers::bigint_value(context, word, kind)
    })
}

/// Write one 64-bit BigInt word, rejecting non-BigInt and out-of-range values.
pub(in crate::entry) fn write(
    this: u64,
    value: u64,
    offset: u64,
    kind: Kind,
    little: bool,
) -> f64 {
    let Some(count) = validate::bytes("buffer", this) else {
        return 0.0;
    };
    let Some(at) = validate::element_offset("offset", offset, count, kind.size()) else {
        return 0.0;
    };
    let (word, lossy) = if kind == Kind::BigInt64 {
        let Some((word, lossy)) = super::super::bigints::bigint_i64(value) else {
            errors::invalid_bigint_type();
            return 0.0;
        };
        (word as u64, lossy)
    } else {
        let Some((word, lossy)) = super::super::bigints::bigint_u64(value) else {
            errors::invalid_bigint_type();
            return 0.0;
        };
        (word, lossy)
    };
    if lossy {
        let expected = if kind == Kind::BigInt64 {
            ">= -(2n ** 63n) and < 2n ** 63n"
        } else {
            ">= 0n and < 2n ** 64n"
        };
        errors::out_of_range_bigint("value", expected, value);
        return 0.0;
    }
    with_current(|context| {
        let Some(view) = view_of(context, this) else {
            return 0.0;
        };
        if let Some(bytes) = window_mut(context, &view) {
            element::write_word(bytes, at, kind, word, little);
        }
        (at + kind.size()) as f64
    })
}
