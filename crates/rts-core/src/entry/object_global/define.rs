//! `[[DefineOwnProperty]]` as the two spellings a program reaches it by.
//!
//! `Object.defineProperty` RAISES a refusal and `Reflect.defineProperty`
//! REPORTS one, and that single difference is the whole reason this is a
//! module: the two used to be one raising function with the reporting spelling
//! answering `true` whenever the target happened to be an object, so a program
//! branching on `Reflect.defineProperty(...)` ended instead of branching.
//!
//! Here rather than in [`super`] because that file is at its ceiling, and
//! apart from [`super::descriptor`] because that one is
//! `ValidateAndApplyPropertyDescriptor` — what a definition is ALLOWED to do —
//! while this is how the answer leaves.

use super::descriptor;
use super::super::with_current;
use crate::value::Value;

/// One property defined from one descriptor, throwing when it is refused.
///
/// Its own function because `Object.create` and `Object.defineProperties` are
/// this in a loop, and a second reading of what a descriptor means is where one
/// of the three learns that `{}` defines `undefined` and the others do not.
///
/// The throw is here rather than in [`descriptor`] because that is the whole
/// difference between this and `Reflect.defineProperty`, which reports the same
/// refusal as `false`.
pub(in crate::entry) fn define(object: u64, name: u64, stated: u64) {
    define_or_report(object, name, stated, Refusal::Raises);
}

/// The same operation REPORTING the refusal instead of raising it.
///
/// `Reflect.defineProperty`'s whole difference from `Object.defineProperty` is
/// this boolean, and it was not expressible: `Reflect.defineProperty` forwarded
/// to the raising spelling and then answered `true` whenever the target was an
/// object, so `Reflect.defineProperty(class K {}, "prototype", {value: {}})`
/// ENDED THE PROGRAM where every runtime answers `false`. A reporting operation
/// that throws is worse than a wrong boolean: a program written to branch on the
/// answer never reaches its own branch.
///
/// `false` also for a descriptor that was not readable, where the throw is the
/// specification's own — `ToPropertyDescriptor` raises for both spellings, which
/// is why the caller still has to ask `throw::in_flight` before it trusts the
/// `false`.
pub(in crate::entry) fn define_reported(object: u64, name: u64, stated: u64) -> bool {
    define_or_report(object, name, stated, Refusal::Reported)
}

/// How a refusal leaves [`define_or_report`].
#[derive(Clone, Copy, PartialEq)]
enum Refusal {
    /// `Object.defineProperty`: a `TypeError` naming the property.
    Raises,
    /// `Reflect.defineProperty`: `false`.
    Reported,
}

/// One definition, with the refusal reported either way the two spellings want.
///
/// Answers whether the property is now what the descriptor said. A `RangeError`
/// and the descriptor-reading throws are NOT refusals and raise for both
/// spellings — the specification puts them before the validation that can say
/// no.
fn define_or_report(object: u64, name: u64, stated: u64, refusal: Refusal) -> bool {
    // A proxy answers with its handler, and it is asked BEFORE the descriptor is
    // read: `Reflect.defineProperty` — the same operation reporting instead of
    // raising — hands the trap the descriptor the program wrote, and reading it
    // here first would run a field's getter once for this check and again inside
    // the handler.
    //
    // The divergence that leaves, named: `ToPropertyDescriptor` does not run for
    // a trapped define, so a handler is handed the object as written rather than
    // the normalised one, and an invalid descriptor is the handler's problem
    // instead of a `TypeError` before it. The forwarding case still validates —
    // it reaches this function again on the target.
    if let Some(key) = with_current(|context| super::super::computed::property_key(context, Value(name)))
        && let Some(accepted) = super::super::proxy::define(object, key, stated)
    {
        // The only difference from `Reflect.defineProperty`, which reports the
        // same refusal as `false`. Rule 8: a trap that threw already has an
        // error on its way out, and a second one here would name this operation
        // for the handler's failure.
        if !accepted && refusal == Refusal::Raises && !super::super::throw::in_flight() {
            super::super::throw::type_error(&format!(
                "'defineProperty' on proxy: trap returned falsish for property '{}'",
                spelled(name)
            ));
        }
        return accepted;
    }
    let Some(wanted) = descriptor::read(stated) else {
        // Already thrown: either the descriptor was not an object, or reading a
        // field of it ran a getter that threw. Rule 8 — the answer is not
        // looked at.
        return false;
    };
    match refusal {
        Refusal::Raises => {
            define_read(object, name, &wanted);
            !super::super::throw::in_flight()
        }
        Refusal::Reported => match descriptor::apply(object, name, &wanted) {
            descriptor::Verdict::Done => true,
            descriptor::Verdict::Refused => false,
            // A raise for both spellings: step 1 of `Reflect.defineProperty`
            // refuses a non-object target before it reaches the validation that
            // could report, so `Reflect.defineProperty(1, "x", {})` is a
            // `TypeError` and not a `false`.
            descriptor::Verdict::NotObject => {
                super::super::throw::type_error("Reflect.defineProperty called on non-object");
                false
            }
            // Still a raise: `Reflect.defineProperty(a, "length", {value: -1})`
            // is a `RangeError` in every runtime, because `ArraySetLength`
            // throws before the validation that could report.
            descriptor::Verdict::BadLength => {
                super::super::throw::range_error("Invalid array length");
                false
            }
        },
    }
}

/// A property name as an error message spells it.
fn spelled(name: u64) -> String {
    with_current(|context| {
        super::super::text::to_text(context, Value(name))
            .and_then(|text| text.to_rust())
            .unwrap_or_default()
    })
}

/// The second half, for a caller that read the descriptor earlier.
///
/// `Object.defineProperties` is that caller, and it has to be: the language
/// reads EVERY descriptor before it defines the first property, so a set whose
/// second descriptor throws leaves the object untouched.
pub(in crate::entry) fn define_read(object: u64, name: u64, wanted: &descriptor::Descriptor) {
    match descriptor::apply(object, name, wanted) {
        descriptor::Verdict::Done => {}
        descriptor::Verdict::Refused => {
            super::super::throw::type_error(&format!("Cannot redefine property: {}", spelled(name)));
        }
        descriptor::Verdict::NotObject => {
            super::super::throw::type_error("Object.defineProperty called on non-object");
        }
        // A `RangeError` and not a `TypeError`, and it is `ArraySetLength`'s
        // own: `a.length = 1.5` is not a property that refuses to be redefined,
        // it is a length that is not a length. `Reflect.defineProperty` raises
        // it too — the verdict/throw split above is about REFUSAL, and this is
        // the specification throwing before it ever gets that far.
        descriptor::Verdict::BadLength => {
            super::super::throw::range_error("Invalid array length");
        }
    }
}
