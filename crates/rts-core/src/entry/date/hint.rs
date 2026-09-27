//! `Date.prototype[Symbol.toPrimitive]` — the one conversion the language lets
//! an object decide for itself, and the one built-in that uses it.
//!
//! # Why this is the whole of the "`date + 1` is text" bug
//!
//! `ToPrimitive` already consults `Symbol.toPrimitive` before `valueOf` and
//! `toString`: [`crate::entry::primitive::to_primitive`] does it, and `+`, the
//! four relational operators, `==`, `String()` and `Number()` all go through
//! there. Nothing about that protocol was missing. What was missing is the
//! METHOD — `Date.prototype` had none — so every hint fell through to
//! `OrdinaryToPrimitive`, where `Hint::Default` behaves as `Hint::Number` and
//! `valueOf` answers first.
//!
//! That is right for every object in the language except this one. `Date` is the
//! single built-in whose default hint is **string**, which is why `d + 1` is text
//! while `+d` is a number, and why `d == d.getTime()` is FALSE — the object
//! converts to its `toString` text, and `ToNumber` of that is `NaN`.
//!
//! # What this method does NOT do, said out loud
//!
//! The specification defines it as `OrdinaryToPrimitive(this, hint)`, which looks
//! `toString` and `valueOf` up ON THE RECEIVER and calls whichever the hint puts
//! first. This reads the receiver's time value directly instead. The difference
//! is visible to exactly one program: one that replaces `Date.prototype.toString`
//! or `Date.prototype.valueOf` and then converts a date implicitly — it sees the
//! ISO text here and its own method in a real engine.
//!
//! It is written this way because `OrdinaryToPrimitive` has one place it lives,
//! inside `entry::primitive`, and that function consults `Symbol.toPrimitive`
//! first — so calling it from here is unbounded recursion, and copying its loop
//! into this file is the rule written twice that this crate's README rule 2 is
//! about. Reading the time value calls no user code at all, which is also why
//! README rule 8 has nothing to ask of this native.

use super::civil::{iso_text, local_string};
use super::support::{self, text_value, time_of};
use super::{Context, undefined_of, with_current};
use crate::value::Value;

/// Hangs the method on `Date.prototype`, under the key a symbol writes.
///
/// # Why `put` rather than `native::install`
///
/// The same seam `string::iterator_method` records: `install` names a method by
/// the key it stores it under, and those two strings differ here. The key is
/// `"@@toPrimitive"` — the encoding [`crate::entry::symbol`] mints, spelled from
/// its own `PREFIX` rather than written out again — while `.name` is
/// `"[Symbol.toPrimitive]"`, which is what a program reads and what the fixture
/// comparing this engine against Node checks.
///
/// `.length` is written here for the same reason `native::install_with_arity`
/// exists for the `Object` statics: this function value IS read rather than only
/// called, and the specification pins its arity at one.
///
/// # Why it answers early when the key is already there
///
/// Because `class_support::made` makes the registration around this idempotent
/// and this has to be too — but not by writing the same thing twice, which is
/// what `collections::alias` can afford and this cannot: that one copies a value
/// the prototype already holds, while this MAKES a callable, so a second run
/// would leave `Date.prototype[Symbol.toPrimitive]` a different object from the
/// one a program had already read.
pub(super) fn install(context: &mut Context, prototype: u32) {
    let key = context.well_known(&format!("{}toPrimitive", super::super::symbol::PREFIX));
    if super::super::objects::read_property(context, prototype, key).is_some() {
        return;
    }
    let method = super::super::native::callable(context, by_hint);
    super::super::native::name_of(context, method, "[Symbol.toPrimitive]");
    if let Some(cell) = Value(method).as_slot() {
        let length = context.well_known("length");
        let one = Value::from_f64(1.0).bits();
        super::super::objects::put(context, cell, length, one);
    }
    super::super::objects::put(context, prototype, key, method);
}

/// `date[Symbol.toPrimitive](hint)`.
///
/// # Why a hint that is not one of the three throws
///
/// Because the specification says so, and because the alternative hides a real
/// mistake: `d[Symbol.toPrimitive]()` with no argument is a program that thinks
/// it asked for something. Answering the number — the shape a default would take
/// — makes that program work here and fail everywhere else.
///
/// The hint is compared as a VALUE and never coerced. `ToString` of it would make
/// `prim.call(d, { toString: () => "number" })` legal, which no engine accepts,
/// and would turn this native into one that calls user code.
extern "C" fn by_hint(_e: u64, this: u64, hint: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let asked = with_current(|context| {
        let text = Value(hint).as_slot().and_then(|slot| context.text_at(slot))?;
        text.to_rust()
    });
    match asked.as_deref() {
        Some("number") => Value::from_f64(time_of(this)).bits(),
        // `"default"` beside `"string"` and not beside `"number"`: this is the
        // whole reason the method exists — see the module documentation.
        //
        // ES2025 §21.4.4.45 calls `OrdinaryToPrimitive(this, "string")` for
        // both hints, which looks `toString` up on the receiver first — the
        // §21.4.4.41 form (`local_string`), not the ISO one. This answered
        // `iso_text` here, so `` `${new Date(0)}` `` and `` `${new
        // Date(0)}` `` printed `1970-01-01T00:00:00.000Z` where Node prints
        // `Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)` —
        // the exact text `Date.prototype.toString` itself already answers, a
        // few lines away in `class.rs`.
        Some("string" | "default") => text_value(local_string(time_of(this))),
        _ => {
            super::super::throw::type_error(
                "Date.prototype[Symbol.toPrimitive] takes a hint of \"number\", \
                 \"string\" or \"default\"",
            );
            with_current(|context| undefined_of(context))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::with_context;
    use crate::value::Singletons;

    /// A context installed for the duration, the way a host installs one.
    fn hosted<T>(body: impl FnOnce() -> T) -> T {
        let singletons = Singletons { undefined: 0, null: 1, hole: 2 };
        let context = Context::new(singletons, crate::value::Kinds::in_declaration_order());
        with_context(context, body).1
    }

    fn text_of(value: u64) -> String {
        with_current(|context| {
            Value(value)
                .as_slot()
                .and_then(|cell| context.text_at(cell))
                .and_then(|text| text.to_rust())
                .expect("a string")
        })
    }

    #[test]
    fn string_and_default_hints_answer_the_long_local_form_not_the_iso_one() {
        // ES2025 §21.4.4.45: `OrdinaryToPrimitive(this, "string")` for BOTH
        // hints, which looks `toString` up first — the long local form, not
        // the ISO one `JSON.stringify`/`toISOString` use. This used to answer
        // `iso_text` for both, so `` `${new Date(0)}` `` printed
        // `1970-01-01T00:00:00.000Z` where Node prints
        // `Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)`.
        hosted(|| {
            with_current(super::super::register_date);
            let undef = with_current(|context| undefined_of(context));
            let cell = with_current(|context| support::receiver(context, undef))
                .expect("Date is registered");
            with_current(|context| support::store(context, cell, 0.0));
            let this = Value::from_slot(cell).bits();
            let string_hint =
                with_current(|context| context.intern_value(crate::text::Str::from_str("string")).bits());
            let default_hint =
                with_current(|context| context.intern_value(crate::text::Str::from_str("default")).bits());

            let expected = local_string(0.0);
            assert_eq!(text_of(by_hint(0, this, string_hint, 0, 0, 0)), expected);
            assert_eq!(text_of(by_hint(0, this, default_hint, 0, 0, 0)), expected);
            assert_ne!(expected, iso_text(0.0), "the two forms must differ, or this test proves nothing");
        });
    }
}
