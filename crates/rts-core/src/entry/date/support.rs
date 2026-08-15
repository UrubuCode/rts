//! What every member of the class needs and none of them is: the clock, the
//! receiver, and the four ways to read or write a time value.
//!
//! Split out of the class for the reason README rule 6 gives, and along the same
//! seam [`super::civil`] follows: nothing here decides a calendar question and
//! nothing there touches a `Context`. What is left in `mod.rs` is the member
//! list, which is the part a reader goes there to find.

use super::civil::{Parts, parts_of};
use super::{Context, TIME, Value, read_property, undefined_of, with_current};
use crate::text::Str;

/// The clock. The one line a wasm target has to supply — see the module doc.
pub(in crate::entry) fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(f64::NAN, |since| since.as_millis() as f64)
}

/// The object to write onto: the one `new` made, or one made here.
pub(super) fn receiver(context: &mut Context, this: u64) -> Option<u32> {
    crate::entry::class_support::receiver(context, this, "Date")
}

/// Writes the time value onto the object.
pub(super) fn store(context: &mut Context, cell: u32, ms: f64) {
    let key = context.well_known(TIME);
    crate::entry::objects::put(context, cell, key, Value::from_f64(ms).bits());
}

/// The time value a receiver carries, `NaN` for anything carrying none.
///
/// `NaN` rather than a refusal, because every getter already answers `NaN` for
/// an invalid date — so a receiver that is not a date takes a path that exists
/// instead of adding a second kind of failure for callers to distinguish.
pub(super) fn time_of(this: u64) -> f64 {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return f64::NAN;
        };
        let key = context.well_known(TIME);
        read_property(context, cell, key)
            .and_then(|found| found.as_f64())
            .unwrap_or(f64::NAN)
    })
}

/// One calendar field of a receiver, `NaN` when the date is invalid.
pub(super) fn field(this: u64, pick: fn(&Parts) -> f64) -> f64 {
    parts_of(time_of(this)).as_ref().map_or(f64::NAN, pick)
}

/// Every field of a receiver, falling back to the epoch's when the date is
/// invalid — the same fallback `setFullYear` already makes, so a setter on an
/// invalid date answers a real one instead of propagating `NaN` forever.
pub(super) fn fields_of(this: u64) -> Parts {
    parts_of(time_of(this)).unwrap_or(parts_of(0.0).expect("epoch has parts"))
}

/// The value the language calls absent, read once per member that needs it.
///
/// What replaced `optional(value, absent, current)`, which every trailing setter
/// argument used to call: a setter no longer asks whether an argument *is*
/// absent, because [`super::fields::given`] answers how many the call carried
/// and a field the call did not reach is simply not written.
pub(super) fn absent() -> u64 {
    with_current(|context| undefined_of(context))
}

/// Stores a computed time value onto the receiver and answers it — what every
/// setter in this file does with its result, pulled out once rather than
/// repeated at each call site.
pub(super) fn commit(this: u64, ms: f64) -> f64 {
    with_current(|context| {
        if let Some(cell) = Value(this).as_slot() {
            store(context, cell, ms);
        }
    });
    ms
}

/// A `String` value holding text that was produced outside any borrow.
pub(super) fn text_value(text: String) -> u64 {
    with_current(|context| context.intern_value(Str::from_str(&text)).bits())
}

/// `null`, spelled here as several modules in this folder spell it, for want of
/// one place low enough to hold it that knows the singleton numbering.
pub(super) fn null_of(context: &Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.null),
    )
}
