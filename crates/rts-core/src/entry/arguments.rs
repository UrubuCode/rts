//! The `arguments` object a non-arrow function sees.
//!
//! # Why this is not [`super::functions::rest_arguments`]
//!
//! Because `arguments` is not an Array and `...rest` is. The emitter used to
//! build both with `rest_arguments`, and the difference was program-visible on
//! the first line that asked: `Array.isArray(arguments)` answered `true` here
//! and `false` in every real engine. It is not a cosmetic disagreement — in
//! this runtime "is an array" IS "the cell carries an elements vector", and an
//! array's prototype is SUBSTITUTED rather than linked (see
//! `super::array_proto::construct`), so an `arguments` built as an array also
//! inherited from `Array.prototype`: `arguments.map` existed, and the language
//! says it does not.
//!
//! # What answers it instead, and what had to be added
//!
//! Nothing did. The nearest is `rest_arguments`, which differs in exactly the
//! way above; the collection step is shared with it —
//! [`super::functions::collected`] — because WHERE the arguments of a running
//! call live is one question with one answer, and only what is built out of
//! them differs.
//!
//! So this builds an ordinary object, through the same
//! `objects::object_new_wide` an object literal goes through: index properties,
//! a `length`, and `Symbol.iterator`. The last is what keeps `[...arguments]`
//! and `Array.from(arguments)` working once the object stops being an array,
//! and it is `Array.prototype.values` — read off the array prototype rather
//! than minted again, for the reason `array_proto::prototype_of` gives about
//! `values` itself: two walks of one sequence is the failure found last.
//!
//! `length` and `Symbol.iterator` are non-enumerable, which is what the
//! specification says and what keeps `for (const k in arguments)` answering the
//! indices alone.
//!
//! What is still NOT here: the mapping between a parameter and its index in
//! sloppy mode (`function f(a) { a = 9; return arguments[0] }` answers `1` here
//! and in Bun and Node alike for a `.ts` module, since a module is strict), and
//! `Symbol.iterator` being writable-per-object is untested. Both need a cell
//! that holds an alias, which this runtime does not have.

use super::rooted::Rooted;
use super::{Context, with_current};
use crate::value::Value;

/// `arguments` — an array-LIKE object over what the caller really passed.
#[rtse::entry]
pub fn arguments_object(a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    with_current(|context| {
        let collected = super::functions::collected(context, 0, a0, a1, a2, a3);
        build(context, collected)
    })
}

/// The object itself, over values the caller has already collected.
///
/// # Why the values are rooted for the whole of it
///
/// Because every step after the allocation allocates: interning `"0"` allocates,
/// and so does the shape transition each `put` takes. A `Vec<u64>` on a Rust
/// frame is invisible to the collector — `super::rooted` is the module that
/// says so, and says it was measured as wrong ANSWERS rather than a crash. The
/// new cell goes into the same list for the same reason: it is named by nothing
/// the collector walks until it is returned.
fn build(context: &mut Context, collected: Vec<u64>) -> u64 {
    // Room for the indices, `length` and `Symbol.iterator`. A wrong count costs
    // a slot and never an answer — see `objects::object_new_wide`.
    let object = super::objects::object_new_wide(context, collected.len() as i64 + 2);
    let Some(cell) = Value(object).as_slot() else {
        return super::objects::undefined_of(context);
    };
    let mut held = Rooted::with(collected);
    held.values().push(object);
    let count = held.len() - 1;

    for at in 0..count {
        let value = held.as_slice()[at];
        let key = context.well_known(&at.to_string());
        super::objects::put(context, cell, key, value);
    }

    let key = context.well_known("length");
    let length = Value::from_f64(count as f64).bits();
    super::objects::put(context, cell, key, length);
    super::native::hidden(context, cell, key);

    // Read off `Array.prototype` rather than minted here: `[...arguments]` and
    // `[...Array.from(arguments)]` walking the same sequence differently is the
    // bug that would be found last.
    if let Some(prototype) = super::array_proto::prototype_of(context) {
        let key = context.well_known(&format!("{}iterator", super::symbol::PREFIX));
        if let Some(values) = super::objects::own_property(context, prototype, key) {
            super::objects::put(context, cell, key, values.bits());
            super::native::hidden(context, cell, key);
        }
    }
    object
}
