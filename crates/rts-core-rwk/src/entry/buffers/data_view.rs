//! `DataView` — a window onto a buffer whose element type is decided per call.
//!
//! # Why the default byte order is BIG-endian
//!
//! Because the specification says so, and it surprises people — every machine
//! this runs on is little-endian, and every typed array over the same bytes reads
//! them that way. `view.getUint16(0)` and `new Uint16Array(buffer)[0]` therefore
//! answer *different numbers* over identical memory, and that is correct.
//!
//! It is not an arbitrary choice being copied. `DataView` exists for data whose
//! layout somebody else fixed — a file header, a network packet — and network
//! byte order is big-endian. The `littleEndian` argument is how a caller asks for
//! the other one, and defaulting it to the platform's order was the rejected
//! alternative: it would make the same program answer differently on two
//! machines, which is the one thing a class for foreign data must not do.
//!
//! # Why sixteen members and not four with a width argument
//!
//! `getInt16` is the name a program writes. A `get(width, signed)` would be one
//! function and nobody's API — and the sixteen here are each one line over
//! [`fetch`] and [`store`], so what is repeated is the name, which is the part
//! that has to be repeated.

use super::element::Kind;
use super::{View, with_current};
use crate::entry::objects::undefined_of;
use crate::value::Value;

/// `DataView`.
#[rtse::class("DataView")]
impl DataView {
    /// `new DataView(buffer, byteOffset?, byteLength?)`.
    ///
    /// An offset or length past the end of the buffer is clamped, where the
    /// language throws a `RangeError`. The same stated gap every refusal in this
    /// layer has: a throw here would end the program rather than reach a handler.
    #[construct]
    fn build(this: u64, buffer: u64, offset: u64, length: u64) -> u64 {
        let offset = super::optional_number(offset);
        let length = super::optional_number(length);
        with_current(|context| {
            let absent = undefined_of(context);
            let Some(cell) = Value(this).as_slot() else {
                return absent;
            };
            let Some(over) = Value(buffer).as_slot() else {
                return absent;
            };
            let Some(size) = context.bytes_at(over).map(Vec::len) else {
                // Not a buffer. `new DataView({})` is a `TypeError`; the object
                // this answers is inert instead, and every read off it is
                // `undefined` — a value the program can notice.
                return absent;
            };
            let start = super::range(size, offset, None).0;
            let count = match length {
                Some(asked) => super::as_count(asked).min(size - start),
                None => size - start,
            };
            super::attach(
                context,
                cell,
                View {
                    buffer: over,
                    offset: start,
                    length: count,
                    kind: Kind::Raw,
                },
            );
            Value::from_slot(cell).bits()
        })
    }

    /// `v.getInt8(byteOffset)` — one byte has no order to choose.
    fn get_int8(this: u64, at: f64) -> f64 {
        fetch(this, at, Kind::Int8, true)
    }

    /// `v.getUint8(byteOffset)`.
    fn get_uint8(this: u64, at: f64) -> f64 {
        fetch(this, at, Kind::Uint8, true)
    }

    /// `v.getInt16(byteOffset, littleEndian?)`.
    fn get_int16(this: u64, at: f64, little: bool) -> f64 {
        fetch(this, at, Kind::Int16, little)
    }

    /// `v.getUint16(byteOffset, littleEndian?)`.
    fn get_uint16(this: u64, at: f64, little: bool) -> f64 {
        fetch(this, at, Kind::Uint16, little)
    }

    /// `v.getInt32(byteOffset, littleEndian?)`.
    fn get_int32(this: u64, at: f64, little: bool) -> f64 {
        fetch(this, at, Kind::Int32, little)
    }

    /// `v.getUint32(byteOffset, littleEndian?)`.
    fn get_uint32(this: u64, at: f64, little: bool) -> f64 {
        fetch(this, at, Kind::Uint32, little)
    }

    /// `v.getFloat32(byteOffset, littleEndian?)`.
    fn get_float32(this: u64, at: f64, little: bool) -> f64 {
        fetch(this, at, Kind::Float32, little)
    }

    /// `v.getFloat64(byteOffset, littleEndian?)`.
    fn get_float64(this: u64, at: f64, little: bool) -> f64 {
        fetch(this, at, Kind::Float64, little)
    }

    /// `v.setInt8(byteOffset, value)`.
    fn set_int8(this: u64, at: f64, value: f64) -> u64 {
        store(this, at, value, Kind::Int8, true)
    }

    /// `v.setUint8(byteOffset, value)`.
    fn set_uint8(this: u64, at: f64, value: f64) -> u64 {
        store(this, at, value, Kind::Uint8, true)
    }

    /// `v.setInt16(byteOffset, value, littleEndian?)`.
    fn set_int16(this: u64, at: f64, value: f64, little: bool) -> u64 {
        store(this, at, value, Kind::Int16, little)
    }

    /// `v.setUint16(byteOffset, value, littleEndian?)`.
    fn set_uint16(this: u64, at: f64, value: f64, little: bool) -> u64 {
        store(this, at, value, Kind::Uint16, little)
    }

    /// `v.setInt32(byteOffset, value, littleEndian?)`.
    fn set_int32(this: u64, at: f64, value: f64, little: bool) -> u64 {
        store(this, at, value, Kind::Int32, little)
    }

    /// `v.setUint32(byteOffset, value, littleEndian?)`.
    fn set_uint32(this: u64, at: f64, value: f64, little: bool) -> u64 {
        store(this, at, value, Kind::Uint32, little)
    }

    /// `v.setFloat32(byteOffset, value, littleEndian?)`.
    fn set_float32(this: u64, at: f64, value: f64, little: bool) -> u64 {
        store(this, at, value, Kind::Float32, little)
    }

    /// `v.setFloat64(byteOffset, value, littleEndian?)`.
    fn set_float64(this: u64, at: f64, value: f64, little: bool) -> u64 {
        store(this, at, value, Kind::Float64, little)
    }
}

/// One read, at a byte offset within the view's own window.
///
/// `NaN` for an offset the width does not fit at, where the language throws a
/// `RangeError`. `NaN` rather than zero because zero is a number the buffer could
/// genuinely have held, and a caller comparing against it cannot tell a read that
/// failed from one that succeeded.
fn fetch(this: u64, at: f64, kind: Kind, little: bool) -> f64 {
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return f64::NAN;
        };
        let Some(bytes) = super::window(context, &view) else {
            return f64::NAN;
        };
        let index = super::as_count(at);
        super::element::read(bytes, index, kind, little).unwrap_or(f64::NAN)
    })
}

/// One write. Answers `undefined`, which is what a `set*` evaluates to.
fn store(this: u64, at: f64, value: f64, kind: Kind, little: bool) -> u64 {
    with_current(|context| {
        let index = super::as_count(at);
        if let Some(view) = super::view_of(context, this)
            && let Some(bytes) = super::window_mut(context, &view)
        {
            // The answer is discarded: a write that did not fit is silently
            // nothing, for the reason `fetch` records about the missing
            // `RangeError`.
            super::element::write(bytes, index, kind, value, little);
        }
        undefined_of(context)
    })
}
