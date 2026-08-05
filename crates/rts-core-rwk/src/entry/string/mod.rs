//! What a string inherits from.
//!
//! # Why a string needs a prototype at all, when `length` did not
//!
//! `"s".length` and `s[0]` are answered directly, by [`super::objects`], because
//! a string cell has no shape and every read of one takes the slow path — so
//! there is nothing for a special case to disagree with. That worked for exactly
//! two properties and stops at the third: `s.trim` has to produce a *callable*,
//! and a special case answering one would be inventing a function per read.
//!
//! So strings inherit, like everything else. The cell has no own prototype —
//! there are as many string cells as there are strings, and giving each a link
//! would be a word per string to record one shared fact — so the chain walk
//! substitutes the one prototype when it reaches a text cell. See
//! [`super::objects::inherited_from`].
//!
//! # Why that makes `String.prototype.mine = v` work
//!
//! It is an ordinary property write on an ordinary object, and every string in
//! the program already inherits from that object. Nothing about extending a
//! built-in is special-cased, which is the point: a program that adds a method
//! to `String.prototype` is doing what the language has always allowed, and an
//! engine that had to be taught about it would have been built wrong.
//!
//! # What is deliberately absent
//!
//! A `String` **wrapper object**. `new String("a")` makes an object whose
//! `typeof` is `"object"` and which compares unequal to `"a"`, and nothing here
//! makes one — `String(x)` converts, which is the spelling programs use. Named
//! rather than half-built, because a wrapper that behaved like a primitive would
//! be wrong in the one way that is hard to find.

mod basic;
pub(super) mod pattern;

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// What every string inherits from, made once.
///
/// Lazily, like the regular-expression prototype and for the same reason: a
/// program that never calls a string method should not spend a cell per built-in
/// out of a region fixed at construction.
pub(super) fn prototype_of(context: &mut Context) -> Option<u32> {
    if let Some(made) = context.string_prototype {
        return Some(made);
    }
    let cell = super::native::plain(context)?;
    // Recorded BEFORE the methods are installed. Installing them interns names,
    // and interning allocates strings — every one of which reaches this function
    // through the chain walk. Setting it afterwards would recurse until the
    // region ran out, which is not a hypothetical: it is what the first version
    // did.
    context.string_prototype = Some(cell);
    super::native::install(context, cell, basic::NATIVES);
    super::native::install(context, cell, pattern::NATIVES);
    Some(cell)
}

/// `String` itself, as the value the name reads.
///
/// A callable with a `prototype` property, so `String.prototype.mine = v`
/// reaches the object strings inherit from and `String.yellow = f` is an
/// ordinary property write on the constructor.
pub(super) fn constructor(context: &mut Context) -> u64 {
    let callable = super::native::callable(context, convert);
    let prototype = match prototype_of(context) {
        Some(cell) => Value::from_slot(cell).bits(),
        None => return undefined_of(context),
    };
    if let Some(cell) = Value(callable).as_slot() {
        let key = context.well_known("prototype");
        super::objects::put(context, cell, key, prototype);
    }
    callable
}

/// `String(x)` — the text of a value.
///
/// Not `new String(x)`, which makes a wrapper object. `construct` hands this a
/// fresh object as its receiver and takes back what it returns; a primitive
/// string is not an object, so `construct` keeps the fresh one — which means
/// `new String("a")` answers a plain object here. A stated divergence, and the
/// less wrong of the two: the alternative is a wrapper that compares equal to a
/// primitive everywhere except where it does not.
extern "C" fn convert(
    _environment: u64,
    _this: u64,
    value: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    with_current(|context| match super::text::to_text(context, Value(value)) {
        Some(text) => context.intern_value(text).bits(),
        // An object, whose `ToString` runs a `toString` an entry point cannot
        // call. The same boundary every conversion in this crate stops at.
        None => undefined_of(context),
    })
}

/// The receiver as code units.
///
/// Units and not bytes, and not characters: a JavaScript string IS a sequence of
/// UTF-16 code units, so every index a method takes or answers counts those.
/// Doing this in bytes makes `"é".indexOf("x")` disagree with `"é".length`,
/// which is the kind of wrong that survives a whole test suite of ASCII.
pub(super) fn units_of(context: &Context, value: u64) -> Option<Vec<u16>> {
    let text = context.text_at(Value(value).as_slot()?)?;
    Some(text.units().collect())
}

/// An argument as text, converting the way the language does.
///
/// `"abc".indexOf(1)` searches for `"1"`, because the specification runs
/// `ToString` on the argument. `None` is an object, whose conversion calls user
/// code — the boundary every conversion here stops at.
pub(super) fn text_of(context: &Context, value: u64) -> Option<Str> {
    super::text::to_text(context, Value(value))
}

/// The same, as code units.
pub(super) fn arg_units(context: &Context, value: u64) -> Option<Vec<u16>> {
    Some(text_of(context, value)?.units().collect())
}

/// Whether an argument was left out.
///
/// Distinguished from a value, because it decides a default rather than being
/// converted: `"abc".slice(1)` ends at the end and `"abc".slice(1, undefined)`
/// does too, while `"abc".slice(1, 0)` is empty.
pub(super) fn absent(context: &Context, value: u64) -> bool {
    value == undefined_of(context)
}

/// A string value over code units.
pub(super) fn answer(context: &mut Context, units: &[u16]) -> u64 {
    context.intern_value(Str::from_utf16(units)).bits()
}

/// The undefined a method answers when there is nothing to answer.
pub(super) fn nothing(context: &Context) -> u64 {
    undefined_of(context)
}

/// An index a method was given, relative to a length.
///
/// Negative counts from the end and clamping is what the specification does at
/// every one of these — `"abc".slice(-99)` is the whole string rather than an
/// error. Written once because five methods need it and five copies is where
/// one of them would clamp differently.
pub(super) fn relative(index: f64, length: usize) -> usize {
    if index.is_nan() {
        return 0;
    }
    let length = length as f64;
    let at = if index < 0.0 { length + index } else { index };
    at.clamp(0.0, length) as usize
}
