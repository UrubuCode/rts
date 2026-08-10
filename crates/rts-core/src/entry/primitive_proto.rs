//! What a primitive receiver borrows to answer a property read.
//!
//! # Why a number needs this and a string does not
//!
//! A string IS a cell here, so `"a".trim` reaches the chain walk and
//! [`super::objects::inherited_from`] substitutes `String.prototype`. A number,
//! a boolean, a symbol and a bigint are not cells — they are the encoding
//! itself — so there is nothing to walk from, and `(5).toFixed` read `undefined`
//! however complete `Number.prototype` was.
//!
//! So the lookup starts on the shared prototype directly. The receiver is still
//! the primitive: [`super::objects::get_property`] calls what it finds with the
//! value it was given, and `toFixed` reads its own `this` through `ToNumber`
//! rather than expecting an object.
//!
//! # Why no wrapper object is made *implicitly*
//!
//! Because a wrapper is observable and this is not. A wrapper built implicitly
//! to answer `(5).toFixed(2)` would have to be unwrapped again for
//! `(5).valueOf() === 5`, and every place that forgot would compare an object
//! against a primitive.
//!
//! `new Number(5)` is the other case, and it is the language's own decision
//! rather than this file's: the program asked for an object. What that object
//! needs is the primitive it stands for — `[[NumberData]]` in the specification
//! — and [`wrap`] and [`unwrap`] are the two ends of it. This file owns them
//! because it already owns "which primitive does this receiver behave as", and
//! a second answer to that question is what would let `valueOf` and
//! `JSON.stringify` disagree about the same object.
//!
//! # Why the dispatch is here rather than in `objects`
//!
//! Four modules answer it — `number` for two of them, `symbol`, and the bigint
//! class — and a match in the property path would make `objects` name all four.
//! One function that asks each is the same set stated once, and it is the file a
//! fifth primitive is added to.

use super::Context;
use crate::value::{Kind, Value};

/// The prototype a primitive receiver reads its methods from.
///
/// `None` for `undefined` and `null`, which have none — which is exactly why
/// `null.x` is a `TypeError`, and answering `undefined` for it is the stated gap
/// every operation here has while a throw cannot find a handler.
///
/// # Why the class registers on demand
///
/// A program writing `(5).toFixed(2)` may never name `Number`, and the
/// registration is what makes the prototype exist. Reaching it through the
/// global object instead would make a property read depend on whether the
/// program happened to mention a global, which is a different answer for the
/// same expression.
pub(super) fn prototype_of(context: &mut Context, value: Value) -> Option<u32> {
    match value.kind() {
        Kind::Float | Kind::Int => registered(context, "Number", super::number::register_number),
        Kind::Bool => registered(context, "Boolean", super::number::register_boolean),
        Kind::Client { tag, .. } if tag == context.kinds.symbol => {
            super::symbol::prototype_of(context)
        }
        Kind::Client { tag, .. } if tag == context.kinds.bigint => {
            registered(context, "BigInt", super::bigint_class::register_big_int_class)
        }
        _ => None,
    }
}

/// A registered class's prototype, registering it if nothing has yet.
fn registered(
    context: &mut Context,
    name: &'static str,
    register: fn(&mut Context) -> u64,
) -> Option<u32> {
    if super::class_support::made(context, name).is_none() {
        register(context);
    }
    Value(super::class_support::prototype(context, name)?).as_slot()
}

/// Records the primitive a freshly constructed wrapper stands for.
///
/// # Why the test is "a `new` is in progress" and not the receiver's class
///
/// Because `class Money extends Number {}` reaches here with `Money` on the
/// target stack, and a check against `Number` itself would leave the subclass
/// instance holding nothing — the exact wrong answer this closes, moved one
/// level down the chain.
///
/// The receiver being a cell is the other half: `Number(5)` called plainly gets
/// `undefined` as its receiver, so the conversion form cannot record anything.
/// What that leaves uncovered is `Number.call(o, 5)` written INSIDE some other
/// constructor, which would mark `o`. Named rather than solved: the calling
/// convention carries no "was this a construct" bit, and inventing one for a
/// spelling no program uses would be a machine change bought by nothing.
pub(in crate::entry) fn wrap(context: &mut Context, this: u64, primitive: u64) {
    if context.new_targets.is_empty() {
        return;
    }
    if let Some(cell) = Value(this).as_slot() {
        context.set_boxed(cell, primitive);
    }
}

/// The primitive a value stands for — itself, unless it is a wrapper object.
///
/// This is `thisNumberValue`/`thisBooleanValue`/`thisStringValue` written once:
/// each of them reads the same slot and differs only in what it does when the
/// slot is absent, which is the caller's decision rather than this one's.
pub(in crate::entry) fn unwrap(context: &Context, value: u64) -> u64 {
    match Value(value).as_slot().and_then(|cell| context.boxed_at(cell)) {
        Some(primitive) => primitive,
        None => value,
    }
}

/// The same, taking its own borrow, for a native that holds none.
pub(in crate::entry) fn unwrapped(value: u64) -> u64 {
    super::with_current(|context| unwrap(context, value))
}

/// A property a primitive answers itself, before any prototype is consulted.
///
/// One case today: `sym.description`. It cannot be a property on the value —
/// a symbol has no cell, which is the whole point — and it cannot be an accessor
/// on the prototype, because an accessor pair is stored beside a cell and the
/// receiver here is not one. So the read answers it, the same shape
/// `"a".length` already has.
pub(super) fn own_property(
    context: &mut Context,
    value: Value,
    key: crate::object::Key,
) -> Option<u64> {
    if super::symbol::is_symbol(context, value.bits()) {
        return super::symbol::property(context, value.bits(), key);
    }
    None
}
