//! `ArrayBuffer` — bytes with no interpretation.
//!
//! The class that owns memory and can do nothing with it. Every read and write is
//! somebody else's: a `DataView` or a typed array over it. That separation is the
//! language's and it is the reason this file is short — an `ArrayBuffer` has a
//! length, and the only operation it has of its own is making another one.

use super::with_current;
use crate::entry::objects::undefined_of;
use crate::value::Value;

/// `ArrayBuffer`.
#[rtse::class("ArrayBuffer", tag)]
impl ArrayBuffer {
    /// `new ArrayBuffer(byteLength)`.
    ///
    /// The bytes are attached to the object `new` already made rather than to a
    /// fresh one, so `class Mine extends ArrayBuffer {}` keeps `Mine.prototype`.
    /// A plain `ArrayBuffer(8)` with no `new` gets a cell of its own instead of a
    /// `TypeError`, which is the tolerance every constructor here settles on: a
    /// throw would end the program, since [`crate::entry::throw`] cannot find a
    /// handler in a caller.
    #[construct]
    fn build(this: u64, length: f64) -> u64 {
        let count = super::as_count(length);
        with_current(|context| {
            match Value(this).as_slot() {
                Some(cell) => {
                    super::install_bytes(context, cell, count);
                    Value::from_slot(cell).bits()
                }
                None => match super::new_buffer(context, count) {
                    Some(cell) => Value::from_slot(cell).bits(),
                    // The region is full and there is no collector to ask — the
                    // answer every allocation in this crate gives.
                    None => undefined_of(context),
                },
            }
        })
    }

    /// `ArrayBuffer.isView(x)` — whether `x` is a `DataView` or a typed array.
    ///
    /// Asked of the view table rather than of a prototype chain, because that is
    /// what the question means: a plain object with `DataView.prototype` grafted
    /// onto it views nothing, and answering `true` for it would hand every caller
    /// a reference to bytes that do not exist.
    #[stat]
    fn is_view(value: u64) -> bool {
        with_current(|context| super::view_of(context, value).is_some())
    }

    /// `b.slice(begin, end)` — a **copy**, in a new buffer.
    ///
    /// The one operation here that copies, and the counterpart to a typed array's
    /// `subarray`, which does not. Both spellings existing is the whole point:
    /// one asks for independent bytes and the other for another window onto the
    /// same ones, and an implementation that made them agree would be wrong for
    /// whichever caller chose deliberately.
    fn slice(this: u64, begin: u64, end: u64) -> u64 {
        // Before any borrow: each of these takes one of its own.
        let begin = super::optional_number(begin);
        let end = super::optional_number(end);
        with_current(|context| {
            let absent = undefined_of(context);
            let Some(cell) = Value(this).as_slot() else {
                return absent;
            };
            let Some(bytes) = context.bytes_at(cell) else {
                return absent;
            };
            let (first, last) = super::range(bytes.len(), begin, end);
            // Copied out before the new buffer is made, because making one takes
            // the byte store mutably and the source slice borrows it.
            let taken = bytes[first..last].to_vec();
            let Some(made) = super::new_buffer(context, taken.len()) else {
                return absent;
            };
            if let Some(destination) = context.bytes_at_mut(made) {
                destination.copy_from_slice(&taken);
            }
            Value::from_slot(made).bits()
        })
    }
}
