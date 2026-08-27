//! Buffer string writes, including the three legacy encoding-specific methods.
//!
//! The modern `write` overload and the legacy `asciiWrite`/`latin1Write`/
//! `utf8Write` methods share the final byte-copy step. Their argument rules stay
//! separate because the modern method reports numeric range errors while the
//! legacy methods report buffer-bounds errors and clamp an oversized length.

use super::super::buffers::{undefined, view_of, window_mut};
use super::super::errors;
use super::super::{Context, native, with_current};
use super::codec;
use super::validate::{self, Shape};
use crate::value::Value;

/// Install the legacy encoding-specific methods on `Buffer.prototype`.
pub(in crate::entry) fn install_aliases(context: &mut Context, prototype: u64) {
    let Some(cell) = Value(prototype).as_slot() else {
        return;
    };
    native::install_with_arity(
        context,
        cell,
        &[
            ("asciiWrite", ascii_write, 1),
            ("latin1Write", latin1_write, 1),
            ("utf8Write", utf8_write, 1),
        ],
    );
}

/// `buf.write(string, offset?, length?, encoding?)`.
pub(in crate::entry) fn write(
    this: u64,
    string: u64,
    offset: u64,
    length: u64,
    encoding: u64,
) -> f64 {
    let Some(count) = validate::bytes("source", this) else {
        return 0.0;
    };
    let Shape::Text(text) = validate::shape_of(string) else {
        errors::invalid_arg_type("string", "string", string);
        return 0.0;
    };
    let shorthand = matches!(validate::shape_of(offset), Shape::Text(_));
    let (offset, length, encoding) = match shorthand {
        true => {
            let trailing = !matches!(validate::shape_of(length), Shape::Absent)
                || !matches!(validate::shape_of(encoding), Shape::Absent);
            if trailing {
                errors::invalid_arg_type("offset", "number", offset);
                return 0.0;
            }
            (undefined(), length, offset)
        }
        false
            if matches!(validate::shape_of(length), Shape::Text(_))
                && matches!(validate::shape_of(encoding), Shape::Absent) =>
        {
            (offset, undefined(), length)
        }
        false => (offset, length, encoding),
    };
    let Some(enc) = validate::encoding(encoding) else {
        return 0.0;
    };
    let Some(start) = validate::write_offset("offset", offset, count) else {
        return 0.0;
    };
    let cap = match validate::shape_of(length) {
        Shape::Absent => count - start,
        _ => match validate::write_offset("length", length, count - start) {
            Some(length) => length,
            None => return 0.0,
        },
    };
    write_bytes(this, &text, start, cap, &enc)
}

/// The ABI entry point for `Buffer.prototype.asciiWrite`.
pub(in crate::entry) extern "C" fn ascii_write(
    _e: u64,
    this: u64,
    string: u64,
    offset: u64,
    length: u64,
    _a3: u64,
) -> u64 {
    legacy_write(this, string, offset, length, "ascii").to_bits()
}

/// The ABI entry point for `Buffer.prototype.latin1Write`.
pub(in crate::entry) extern "C" fn latin1_write(
    _e: u64,
    this: u64,
    string: u64,
    offset: u64,
    length: u64,
    _a3: u64,
) -> u64 {
    legacy_write(this, string, offset, length, "latin1").to_bits()
}

/// The ABI entry point for `Buffer.prototype.utf8Write`.
pub(in crate::entry) extern "C" fn utf8_write(
    _e: u64,
    this: u64,
    string: u64,
    offset: u64,
    length: u64,
    _a3: u64,
) -> u64 {
    legacy_write(this, string, offset, length, "utf8").to_bits()
}

fn legacy_write(this: u64, string: u64, offset: u64, length: u64, encoding: &str) -> f64 {
    let Some(count) = validate::bytes("source", this) else {
        return 0.0;
    };
    let Shape::Text(text) = validate::shape_of(string) else {
        errors::invalid_arg_type("string", "string", string);
        return 0.0;
    };
    let start = match validate::shape_of(offset) {
        Shape::Absent => 0,
        _ => match validate::legacy_offset("offset", offset, count) {
            Some(start) => start,
            None => return 0.0,
        },
    };
    let cap = match validate::shape_of(length) {
        Shape::Absent => count - start,
        _ => match validate::legacy_length(length, count - start) {
            Some(length) => length,
            None => return 0.0,
        },
    };
    write_bytes(this, &text, start, cap, encoding)
}

fn write_bytes(this: u64, text: &str, start: usize, cap: usize, encoding: &str) -> f64 {
    with_current(|context| {
        let Some(view) = view_of(context, this) else {
            return 0.0;
        };
        let bytes = codec::encode(text, encoding).unwrap_or_default();
        let mut count = bytes.len().min(cap).min(view.count() - start);
        if encoding == "utf16le" {
            count -= count % 2;
        }
        if let Some(destination) = window_mut(context, &view) {
            destination[start..start + count].copy_from_slice(&bytes[..count]);
        }
        count as f64
    })
}
