//! `ArrayBuffer` — bytes with no interpretation.
//!
//! The class that owns memory and can do nothing with it. Every read and write is
//! somebody else's: a `DataView` or a typed array over it. That separation is the
//! language's and it is the reason this file is short — an `ArrayBuffer` has a
//! length, and the only operation it has of its own is making another one.

use super::with_current;
use crate::entry::objects::undefined_of;
use crate::entry::{class_support, native, objects};
use crate::text::Str;
use crate::value::Value;

/// Installs `ArrayBuffer`, and its `Symbol.species`.
///
/// A wrapper around the generated registration rather than a line in
/// [`crate::entry::global`]'s match arm, because that arm is not the only
/// caller: `super::new_buffer` registers the class behind a program's back so
/// that `new Uint8Array(8)` works in a program that never wrote the word
/// `ArrayBuffer`, and a species installed only at the global would have been
/// missing on exactly that path.
///
/// `get [Symbol.species] { return this; }` is the specification's own definition
/// here, which is why this calls [`crate::entry::native::species`] instead of
/// being a function of its own — see its documentation for why all of them are
/// one function.
pub(in crate::entry) fn array_buffer(context: &mut crate::entry::Context) -> u64 {
    let made = register_array_buffer(context);
    crate::entry::native::species(context, made);
    // `resizable` and `maxByteLength` as prototype accessors. A resizable buffer
    // also carries them as own slots (see [`resizable_max`]); every other buffer
    // answers through these, which keeps a plain `new ArrayBuffer(n)` and the
    // internal buffer of every typed array from paying two more properties.
    if let Some(cell) = class_support::prototype(context, "ArrayBuffer").and_then(|v| Value(v).as_slot()) {
        native::getter(context, cell, "resizable", resizable_getter as native::Native);
        native::getter(context, cell, "maxByteLength", max_byte_length_getter as native::Native);
    }
    made
}

/// The ceiling of a resizable buffer, or `None` for a fixed-length (or detached) one.
///
/// A resizable buffer is the only kind that carries own `resizable` and
/// `maxByteLength` slots, so their presence IS the state; detaching rewrites
/// them to `false` / `0` (see `detach.rs`), which makes this `None` afterwards.
pub(super) fn resizable_max(context: &mut crate::entry::Context, cell: u32) -> Option<usize> {
    let on = context.well_known("resizable");
    let held = objects::own_property(context, cell, on)?;
    if held.bits() != Value::from_bool(true).bits() {
        return None;
    }
    let max = context.well_known("maxByteLength");
    objects::own_property(context, cell, max)?.as_f64().map(|n| n as usize)
}

/// Marks a buffer resizable with this ceiling.
fn make_resizable(context: &mut crate::entry::Context, cell: u32, max: usize) {
    super::flag(context, cell, "resizable", true);
    super::stamp(context, cell, "maxByteLength", max as f64);
}

fn is_buffer(context: &crate::entry::Context, cell: u32) -> bool {
    context.bytes_at(cell).is_some()
}

extern "C" fn resizable_getter(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let answer = with_current(|context| {
        let cell = Value(this).as_slot()?;
        if !is_buffer(context, cell) {
            return None;
        }
        Some(Value::from_bool(resizable_max(context, cell).is_some()).bits())
    });
    match answer {
        Some(bits) => bits,
        None => {
            crate::entry::throw::type_error("Receiver is not an ArrayBuffer (get resizable)");
            super::undefined()
        }
    }
}

extern "C" fn max_byte_length_getter(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let answer = with_current(|context| {
        let cell = Value(this).as_slot()?;
        if !is_buffer(context, cell) {
            return None;
        }
        let name = if resizable_max(context, cell).is_some() { "maxByteLength" } else { "byteLength" };
        let key = context.well_known(name);
        objects::own_property(context, cell, key).map(|held| held.bits())
    });
    match answer {
        Some(bits) => bits,
        None => {
            crate::entry::throw::type_error("Receiver is not an ArrayBuffer (get maxByteLength)");
            super::undefined()
        }
    }
}

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
    fn build(this: u64, length: f64, options: u64) -> u64 {
        let Some(count) = super::to_index(length) else {
            crate::entry::throw::range_error("Array buffer allocation failed");
            return super::undefined();
        };
        // `{ maxByteLength }`: absent or `undefined` is a fixed-length buffer.
        let mut ceiling = None;
        if Value(options).as_slot().is_some() {
            let name = with_current(|context| context.intern_value(Str::from_str("maxByteLength")).bits());
            let held = crate::entry::computed::get_indexed(options, name);
            if held != super::undefined() {
                match super::to_index(crate::entry::class_support::to_number(held)) {
                    Some(max) if max >= count => ceiling = Some(max),
                    _ => {
                        crate::entry::throw::range_error("Invalid array buffer max length");
                        return super::undefined();
                    }
                }
            }
        }
        if Value(this).as_slot().is_none() {
            crate::entry::throw::type_error("Constructor ArrayBuffer requires 'new'");
            return super::undefined();
        }
        with_current(|context| match Value(this).as_slot() {
            Some(cell) => {
                super::install_bytes(context, cell, count);
                if let Some(max) = ceiling {
                    make_resizable(context, cell, max);
                }
                Value::from_slot(cell).bits()
            }
            None => undefined_of(context),
        })
    }

    /// `b.resize(n)` — grows (zero-filled) or shrinks a resizable buffer in place.
    fn resize(this: u64, length: u64) -> u64 {
        // `ToIndex(undefined)` is 0, so an absent argument resizes to nothing.
        let wanted = super::optional_number(length).unwrap_or(0.0);
        // 0 ok, 1 not resizable, 2 bad index or over the ceiling.
        let outcome = with_current(|context| {
            let Some(cell) = Value(this).as_slot() else { return 1 };
            if !is_buffer(context, cell) {
                return 1;
            }
            let Some(max) = resizable_max(context, cell) else { return 1 };
            let Some(size) = super::to_index(wanted) else { return 2 };
            if size > max {
                return 2;
            }
            if let Some(bytes) = context.bytes_at_mut(cell) {
                // `Vec::resize` truncates and zero-fills, which is exactly the
                // language: shrinking forgets the tail, growing again does not
                // bring it back.
                bytes.resize(size, 0);
            }
            // The address may have moved and the window of a fixed view may no
            // longer fit: every typed-array view goes back to the runtime path,
            // which resolves bytes per access. Blunt, like detaching.
            context.region.runs_mut().retract_typed();
            super::stamp(context, cell, "byteLength", size as f64);
            0
        });
        match outcome {
            0 => {}
            1 => crate::entry::throw::type_error("Method ArrayBuffer.prototype.resize called on incompatible receiver"),
            _ => crate::entry::throw::range_error("ArrayBuffer.prototype.resize: Invalid length parameter"),
        }
        super::undefined()
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
        // A detached buffer has no bytes to copy. Raised outside the borrow:
        // building the error runs the program's own constructor.
        if with_current(|context| Value(this).as_slot().is_some_and(|cell| context.buffer_detached(cell))) {
            crate::entry::throw::type_error(
                "Cannot perform ArrayBuffer.prototype.slice on a detached ArrayBuffer",
            );
            return super::undefined();
        }
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
        transferred(this, length, "transfer", true)
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
        transferred(this, length, "transferToFixedLength", false)
    }
}

/// The bytes moved into a new buffer, leaving this one detached.
fn transferred(this: u64, length: u64, method: &str, keep_resizable: bool) -> u64 {
    let wanted = super::optional_number(length);
    // `Err(true)` is a RangeError, `Err(false)` a TypeError.
    let taken: Result<(Vec<u8>, Option<usize>), bool> = with_current(|context| {
        let cell = Value(this).as_slot().ok_or(false)?;
        // ToIndex runs BEFORE the detached check, so a bad length is a
        // RangeError on a detached buffer too.
        let size = match wanted {
            Some(number) => Some(super::to_index(number).ok_or(true)?),
            None => None,
        };
        if context.buffer_detached(cell) {
            return Err(false);
        }
        let max = if keep_resizable { resizable_max(context, cell) } else { None };
        let bytes = context.bytes_at(cell).ok_or(false)?;
        let wanted = size.unwrap_or(bytes.len());
        if max.is_some_and(|max| wanted > max) {
            return Err(true);
        }
        let mut moved = vec![0u8; wanted];
        let carried = wanted.min(bytes.len());
        moved[..carried].copy_from_slice(&bytes[..carried]);
        Ok((moved, max))
    });
    let (taken, ceiling) = match taken {
        Ok(found) => found,
        Err(true) => {
            crate::entry::throw::range_error(&format!("Invalid array buffer length (ArrayBuffer.prototype.{method})"));
            return super::undefined();
        }
        Err(false) => {
            crate::entry::throw::type_error(&format!("Cannot perform ArrayBuffer.prototype.{method} on a detached ArrayBuffer"));
            return super::undefined();
        }
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
        if let Some(max) = ceiling {
            make_resizable(context, made, max);
        }
        Value::from_slot(made).bits()
    })
}
