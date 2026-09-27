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

    // The index spelled on the stack: `at.to_string()` was a `String` per
    // argument per call. It stays a NAME key and not `Key::Index`, because a
    // plain object reads its indices through the interned text — `Key::Index`
    // put here was invisible to `arguments[0]` and to a spread, measured
    // before this comment was written.
    let mut digits = [0u8; 10];
    for at in 0..count {
        let value = held.as_slice()[at];
        let spelled = spell(at as u32, &mut digits);
        let key = context.well_known(spelled);
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
        let key = context.well_known(super::symbol::ITERATOR);
        if let Some(values) = super::objects::own_property(context, prototype, key) {
            super::objects::put(context, cell, key, values.bits());
            super::native::hidden(context, cell, key);
        }
    }

    // `Symbol.toStringTag`, so `Object.prototype.toString.call(arguments)`
    // answers `[object Arguments]`.
    //
    // The language gives this one an internal class rather than a tag property,
    // and this engine has no slot to record that in: an arguments object is an
    // ordinary object carrying indices, a `length` and an iterator, which is
    // precisely why `object_proto`'s per-kind table cannot tell it from any
    // other object and answered `[object Object]`. A real tag is the one
    // spelling available, and it is observable in the direction that matters —
    // the value is right, and the extra own key it costs is non-enumerable.
    //
    // The cost is a property per arguments object rather than one on a shared
    // prototype, and that is not a choice: these inherit from
    // `Object.prototype`, so a tag installed there would label every object in
    // the program.
    // Both cached: this runs once per call of any function that mentions
    // `arguments`, and it formatted two symbol spellings, hashed them, and
    // interned a fresh "Arguments" cell every time — about 1 500 ns for an
    // object whose reader usually wants `length`.
    let tag = context.well_known(super::symbol::TO_STRING_TAG);
    let value = context.well_known_text("Arguments");
    super::objects::put(context, cell, tag, value);
    super::native::hidden(context, cell, tag);
    object
}

/// The decimal digits of `index`, written into `digits` from the end.
fn spell(index: u32, digits: &mut [u8; 10]) -> &str {
    let mut at = digits.len();
    let mut left = index;
    loop {
        at -= 1;
        digits[at] = b'0' + (left % 10) as u8;
        left /= 10;
        if left == 0 {
            break;
        }
    }
    std::str::from_utf8(&digits[at..]).expect("decimal digits are ASCII")
}

/// How many arguments the running call was given, for a body that reads
/// `arguments.length` and nothing else of the object — the count the object's
/// `length` would carry, from the same record `build` reads, with no object.
///
/// `emit/light_arguments.rs` says which bodies qualify and why the object is
/// unobservable there. The count is the calling convention's: a counted call
/// left it in `pending_counts`, a call past the slots left the vector, and a
/// call that left neither is measured by its trailing `undefined`s — the same
/// three answers `functions::collected` gives, so the light form and the object
/// cannot disagree about a length.
#[rtse::entry]
pub fn arguments_count(a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    with_current(|context| Value::from_f64(count_of(context, [a0, a1, a2, a3]) as f64).bits())
}

/// The argument at `index` of the running call, for a body that reads
/// `arguments[e]` and nothing else of the object.
///
/// An index that names a slot is answered from the slots; any other key — a
/// string, a fraction, `"length"`, a symbol — builds the object and reads it,
/// so every spelling the program can write answers what the object would.
#[rtse::entry]
pub fn argument_slot(a0: u64, a1: u64, a2: u64, a3: u64, index: u64) -> u64 {
    let given = [a0, a1, a2, a3];
    let answered = with_current(|context| {
        let position = Value(index).numeric()?;
        if position.fract() != 0.0 || position < 0.0 || position >= u32::MAX as f64 {
            return None;
        }
        let at = position as usize;
        let count = count_of(context, given);
        let absent = super::objects::undefined_of(context);
        if at >= count {
            return Some(absent);
        }
        if let Some(vector) = context.pending_arguments.last().copied()
            && let Some(cell) = Value(vector).as_slot()
            && let Some(elements) = context.elements_at(cell)
        {
            return Some(match elements.get(at).copied() {
                Some(value) if !super::array::is_hole(context, value) => value,
                _ => absent,
            });
        }
        given.get(at).copied()
    });
    match answered {
        Some(value) => value,
        None => {
            // Not a slot's name: the object, and the read the program wrote.
            let object = with_current(|context| {
                let collected = super::functions::collected(context, 0, a0, a1, a2, a3);
                build(context, collected)
            });
            super::computed::get_indexed(object, index)
        }
    }
}

/// The running call's argument count, by the three answers `functions::collected`
/// gives, without collecting.
fn count_of(context: &Context, given: [u64; 4]) -> usize {
    if let Some(vector) = context.pending_arguments.last().copied()
        && let Some(cell) = Value(vector).as_slot()
    {
        return context.elements_at(cell).map_or(0, |elements| elements.len());
    }
    if let Some(count) = context.pending_counts.last().copied().flatten() {
        return count.min(given.len());
    }
    let absent = super::objects::undefined_of(context);
    let mut real = given.len();
    while real > 0 && given[real - 1] == absent {
        real -= 1;
    }
    real
}
