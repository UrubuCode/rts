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
//! # Why twenty members and not four with a width argument
//!
//! `getInt16` is the name a program writes. A `get(width, signed)` would be one
//! function and nobody's API — and the twenty here are each one line over
//! [`fetch`] and [`store`], or over [`fetch_big`] and [`store_big`], so what is
//! repeated is the name, which is the part that has to be repeated.
//!
//! Four of them answer and take a **bigint** rather than a number, and that is
//! the one split the pair cannot absorb: it is a difference in the TYPE of the
//! value, not in its width. [`fetch_big`] states why.

use super::element::Kind;
use super::{View, with_current};
use crate::entry::objects::undefined_of;
use crate::value::Value;

/// `DataView`.
#[rtse::class("DataView", tag)]
impl DataView {
    /// `new DataView(buffer, byteOffset?, byteLength?)`.
    ///
    /// Something that is not an `ArrayBuffer` is a `TypeError`; an offset or
    /// length that is not an index, or that runs past the buffer, is a
    /// `RangeError`. Both are raised after the borrow ends — the error is built
    /// by calling the program's own constructor, which needs the context.
    #[construct]
    fn build(this: u64, buffer: u64, offset: u64, length: u64) -> u64 {
        let offset = super::optional_number(offset);
        let length = super::optional_number(length);
        let built = with_current(|context| {
            let Some(cell) = Value(this).as_slot() else {
                return Ok(undefined_of(context));
            };
            let Some((over, size)) = Value(buffer)
                .as_slot()
                .and_then(|over| context.bytes_at(over).map(|bytes| (over, bytes.len())))
            else {
                return Err(Refusal::NotABuffer);
            };
            let start = super::to_index(offset.unwrap_or(0.0)).ok_or(Refusal::Offset)?;
            if start > size {
                return Err(Refusal::Offset);
            }
            let count = match length {
                Some(asked) => {
                    let asked = super::to_index(asked).ok_or(Refusal::Length)?;
                    if start + asked > size {
                        return Err(Refusal::Length);
                    }
                    asked
                }
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
            Ok(Value::from_slot(cell).bits())
        });
        let refusal = match built {
            Ok(made) => return made,
            Err(refusal) => refusal,
        };
        match refusal {
            Refusal::NotABuffer => crate::entry::throw::type_error(
                "First argument to DataView constructor must be an ArrayBuffer",
            ),
            Refusal::Offset => crate::entry::throw::range_error(
                "Start offset is outside the bounds of the buffer",
            ),
            Refusal::Length => crate::entry::throw::range_error("Invalid DataView length"),
        }
        super::undefined()
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

    /// `v.getBigInt64(byteOffset, littleEndian?)`.
    fn get_big_int64(this: u64, at: f64, little: bool) -> u64 {
        fetch_big(this, at, Kind::BigInt64, little)
    }

    /// `v.getBigUint64(byteOffset, littleEndian?)`.
    fn get_big_uint64(this: u64, at: f64, little: bool) -> u64 {
        fetch_big(this, at, Kind::BigUint64, little)
    }

    /// `v.setBigInt64(byteOffset, value, littleEndian?)`.
    fn set_big_int64(this: u64, at: f64, value: u64, little: bool) -> u64 {
        store_big(this, at, value, Kind::BigInt64, little)
    }

    /// `v.setBigUint64(byteOffset, value, littleEndian?)`.
    fn set_big_uint64(this: u64, at: f64, value: u64, little: bool) -> u64 {
        store_big(this, at, value, Kind::BigUint64, little)
    }
}

/// One read, at a byte offset within the view's own window.
///
/// An offset the width does not fit at raises a `RangeError` in [`position`];
/// the `NaN` answered then is never seen, because the throw is in flight.
fn fetch(this: u64, at: f64, kind: Kind, little: bool) -> f64 {
    let Some(index) = position(this, at, kind) else {
        return f64::NAN;
    };
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return f64::NAN;
        };
        let Some(bytes) = super::window(context, &view) else {
            return f64::NAN;
        };
        super::element::read(bytes, index, kind, little).unwrap_or(f64::NAN)
    })
}

/// One sixty-four-bit read, answered as a **bigint**.
///
/// # Why these four are not [`fetch`] with a wider kind
///
/// Because the answer has a different TYPE. Every other accessor here answers a
/// number, and at sixty-four bits a double stops being able to carry the value
/// — which is the whole reason the language made `BigInt64Array`'s elements
/// bigints rather than numbers. So the signature differs, and a `f64` in it
/// would round exactly the range these methods exist to reach.
///
/// The bytes are gathered by the same [`super::element::word_at`] the typed
/// arrays use, so a `DataView` and a `BigInt64Array` over one buffer cannot
/// come to disagree about byte three.
///
/// An offset the width does not fit at raises in [`position`], as [`fetch`]'s.
fn fetch_big(this: u64, at: f64, kind: Kind, little: bool) -> u64 {
    let Some(index) = position(this, at, kind) else {
        return super::undefined();
    };
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = super::view_of(context, this) else {
            return absent;
        };
        let Some(bytes) = super::window(context, &view) else {
            return absent;
        };
        let Some(word) = super::element::word_at(bytes, index, kind, little) else {
            return absent;
        };
        // The read is finished with the bytes, which is what lets the borrow
        // become mutable — allocating the digits needs it.
        super::bigint_value(context, word, kind)
    })
}

/// One sixty-four-bit write, taking a **bigint** and nothing else.
///
/// A number is refused rather than coerced, which is the language's rule in
/// both directions and [`super`]'s stated answer to it: the write is dropped
/// and the bytes keep what they held. Coercing would make a program no other
/// engine accepts run and answer something.
fn store_big(this: u64, at: f64, value: u64, kind: Kind, little: bool) -> u64 {
    let Some(index) = position(this, at, kind) else {
        return super::undefined();
    };
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return undefined_of(context);
        };
        // Read out of the digit slab before the window is taken mutably: both
        // want the context, and the word is what crosses between them.
        let word = super::bigint_word(context, value, kind);
        if let Some(word) = word
            && let Some(bytes) = super::window_mut(context, &view)
        {
            super::element::write_word(bytes, index, kind, word, little);
        }
        undefined_of(context)
    })
}

/// One write. Answers `undefined`, which is what a `set*` evaluates to.
fn store(this: u64, at: f64, value: f64, kind: Kind, little: bool) -> u64 {
    let Some(index) = position(this, at, kind) else {
        return super::undefined();
    };
    with_current(|context| {
        if let Some(view) = super::view_of(context, this)
            && let Some(bytes) = super::window_mut(context, &view)
        {
            // The answer is discarded: `position` already refused an offset
            // the width does not fit at.
            super::element::write(bytes, index, kind, value, little);
        }
        undefined_of(context)
    })
}

/// Why `new DataView(…)` refused, carried out of the borrow to be raised.
enum Refusal {
    NotABuffer,
    Offset,
    Length,
}

/// The byte offset a get or set may use, or `None` once a `RangeError` has been
/// raised for it.
///
/// Every accessor asks this first, OUTSIDE the context borrow, because raising
/// constructs the error through the program's own `RangeError`. The width is
/// checked against the view's own window — not the buffer's — which is what
/// makes `new DataView(buf, 3, 4).getUint8(4)` a throw even though byte 7 of the
/// buffer exists. A receiver that is not a view is left to the caller, which
/// answers `NaN` or `undefined` as it did before.
fn position(this: u64, at: f64, kind: Kind) -> Option<usize> {
    let window = with_current(|context| super::view_of(context, this).map(|view| view.length));
    let index = super::to_index(at);
    match (index, window) {
        (_, None) => Some(index.unwrap_or(0)),
        (Some(index), Some(length)) if index.checked_add(kind.size()).is_some_and(|end| end <= length) => {
            Some(index)
        }
        _ => {
            crate::entry::throw::range_error("Offset is outside the bounds of the DataView");
            None
        }
    }
}
