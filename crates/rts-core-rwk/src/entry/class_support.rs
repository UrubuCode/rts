//! What a declared class needs at run time, and nothing a class declares.
//!
//! # Why the registry is a list and not a field per class
//!
//! `regexp_prototype`, `string_prototype` and `array_prototype` are fields on
//! the context because each has exactly one reader that knows its name. A
//! declared class does not: `#[rtse::class]` expands to code that has to find
//! *its own* prototype without the context having grown a field for it, and a
//! field per class would mean the attribute could not add one without editing
//! the struct — which is the "a proc macro cannot see its neighbours" limit
//! showing up as a build error instead of as a design.
//!
//! So the registry is keyed by the name the class declares, and looked up by a
//! linear scan. That is the right shape at this size for the reason
//! [`super::table`] gives about its own list, and the scan is over a handful of
//! entries reached once per class per program — the second read of `Error` is
//! answered by the global object's own property, not by this.
//!
//! # Why the coercions are here rather than in the expansion
//!
//! Because each one takes a borrow of the context and gives it back, and the
//! generated wrapper's whole safety argument is that the author's body runs with
//! none held. Expanding the borrow inline would put a `with_current` in every
//! wrapper and make that argument something a reader has to re-derive per
//! member.

use super::{Context, with_current};
use crate::value::Value;

/// What a class registration produced, remembered so the second read is the
/// same object.
///
/// The prototype is kept beside the constructor because something other than the
/// class asks for it: the chain walk substitutes `Function.prototype` for a
/// callable that has no link of its own, exactly as it substitutes
/// `String.prototype` for a text cell.
pub(super) struct Registered {
    /// The name JavaScript knows it by.
    pub(super) name: &'static str,
    /// The value the global name reads.
    pub(super) made: u64,
    /// What instances inherit from — the object itself, for a namespace.
    pub(super) prototype: u64,
}

/// What a class registered as, if it has been registered.
pub(in crate::entry) fn made(context: &Context, name: &str) -> Option<u64> {
    context
        .classes
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.made)
}

/// What instances of a registered class inherit from.
pub(in crate::entry) fn prototype(context: &Context, name: &str) -> Option<u64> {
    context
        .classes
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.prototype)
}

/// Records a registration.
///
/// Called **before** the members are installed, and that ordering is the point:
/// installing interns names, interning allocates, and an allocation can reach
/// back into the same registration. Recording afterwards is what made the first
/// version of `string::prototype_of` recurse until the region ran out.
pub(in crate::entry) fn record(
    context: &mut Context,
    name: &'static str,
    made: u64,
    prototype: u64,
) {
    context.classes.push(Registered {
        name,
        made,
        prototype,
    });
}

/// A constant a class declares, in the two spellings a property can hold
/// without running anything.
///
/// Two rather than one tagged value, because a `Value` is a word whose meaning
/// depends on a context that does not exist while the list is a `const`. Text in
/// particular has to be interned, which allocates — so what the declaration
/// carries is the source form and this is where it becomes a value.
pub(in crate::entry) enum Constant {
    /// `Math.PI`, `Number.MAX_SAFE_INTEGER`.
    Number(f64),
    /// `Error.prototype.name`.
    Text(&'static str),
}

/// Hangs constants on an object, by name.
///
/// Real properties rather than answers the runtime invents, for the reason
/// `regex::describe` records: compiled code reading a property that is in the
/// layout never asks the runtime at all, so a value only the slow path knows
/// about is one the fast path disagrees with the moment it starts working.
pub(in crate::entry) fn constants(context: &mut Context, cell: u32, constants: &[(&str, Constant)]) {
    for (name, held) in constants {
        let value = match held {
            Constant::Number(number) => Value::from_f64(*number).bits(),
            Constant::Text(text) => context
                .intern_value(crate::text::Str::from_str(text))
                .bits(),
        };
        let key = context.well_known(name);
        super::objects::put(context, cell, key, value);
    }
}

/// `ToNumber` of an argument.
///
/// An object is converted by its `valueOf` first, outside any borrow, because
/// that is user code. It used to answer `NaN` there with a comment calling it
/// "the answer such an object would have produced anyway" — which was true of
/// `{}` and false of every object that defines a `valueOf`, so
/// `Number({ valueOf() { return 5 } })` was `NaN`.
///
/// `NaN` remains for what genuinely does not convert: an object whose two
/// methods both answer objects, and a symbol.
pub(in crate::entry) fn to_number(value: u64) -> f64 {
    let value = super::primitive::to_primitive(value, crate::coerce::Hint::Number);
    with_current(|context| super::operators::as_number(context, Value(value)).unwrap_or(f64::NAN))
}

/// `ToBoolean` of an argument.
pub(in crate::entry) fn to_boolean(value: u64) -> bool {
    super::primitives::to_boolean(value)
}

