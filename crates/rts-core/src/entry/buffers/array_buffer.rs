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

    /// `b.transfer(newLength)` — the bytes move to a NEW buffer and this one is
    /// detached.
    ///
    /// # Why this is not `slice` with a detach after it
    ///
    /// Because the two differ in what the caller may assume afterwards. `slice`
    /// leaves the source readable, so a program holding it keeps a second copy
    /// of however many megabytes; `transfer` exists to say that it does not, and
    /// the detach is the operation rather than a tidy-up. A version that copied
    /// and left the source attached would run every program that uses it and be
    /// wrong about the one thing it was called for.
    ///
    /// A shorter length truncates and a longer one zero-fills, which is the
    /// language's rule and is why the new length is read before the copy.
    fn transfer(this: u64, length: u64) -> u64 {
        transferred(this, length)
    }

    /// `b.transferToFixedLength(newLength)`.
    ///
    /// The same operation, and here literally so: it differs from [`transfer`]
    /// only in that the answer is never resizable, and nothing this engine makes
    /// is resizable yet. Present under its own name rather than absent, because
    /// the two spellings do the same thing to the SOURCE — it is detached either
    /// way — and a program that called the wrong one of two would otherwise keep
    /// reading bytes it had given away.
    fn transfer_to_fixed_length(this: u64, length: u64) -> u64 {
        transferred(this, length)
    }
}

/// The bytes moved into a new buffer, leaving this one detached.
fn transferred(this: u64, length: u64) -> u64 {
    let wanted = super::optional_number(length);
    let taken = with_current(|context| {
        let cell = Value(this).as_slot()?;
        if context.buffer_detached(cell) {
            return None;
        }
        let bytes = context.bytes_at(cell)?;
        let wanted = match wanted {
            // `ToIndex` of the argument, and a negative one is a `RangeError`
            // the caller below raises — `None` here would be indistinguishable
            // from an already-detached buffer.
            Some(number) if number >= 0.0 => number as usize,
            Some(_) => return None,
            None => bytes.len(),
        };
        let mut moved = vec![0u8; wanted];
        let carried = wanted.min(bytes.len());
        moved[..carried].copy_from_slice(&bytes[..carried]);
        Some(moved)
    });
    let Some(taken) = taken else {
        crate::entry::throw::type_error("ArrayBuffer is detached");
        return super::undefined();
    };
    // The source is detached BEFORE the answer is made: allocating takes the
    // byte store mutably, and a program that observed the order would see the
    // detach either way. Doing it first keeps the failure mode honest — if the
    // region is full, the caller has lost the bytes rather than kept two copies.
    super::detach::detach_buffer(this);
    with_current(|context| {
        let Some(made) = super::new_buffer(context, taken.len()) else {
            return undefined_of(context);
        };
        if let Some(destination) = context.bytes_at_mut(made) {
            destination.copy_from_slice(&taken);
        }
        Value::from_slot(made).bits()
    })
}
