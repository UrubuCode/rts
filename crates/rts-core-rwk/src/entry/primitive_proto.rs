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
//! # Why no wrapper object is made
//!
//! Because a wrapper is observable and this is not. `new Number(5)` making an
//! object is the language's own decision and a separate one; a wrapper built
//! *implicitly* to answer `(5).toFixed(2)` would have to be unwrapped again for
//! `(5).valueOf() === 5`, and every place that forgot would compare an object
//! against a primitive.
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
