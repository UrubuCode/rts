//! `f.call(thisArg, …)` and `f.apply(thisArg, list)`, reached directly.
//!
//! `f.call(null, a)` cost 165 ns and `f.apply(null, [a])` 235 against 4 for
//! `f(a)` (`bench/analytic.ts`, release, 2026-09-27): a property read up the
//! chain to `Function.prototype`, a native dispatch, the argument vector the
//! native rebuilds from its own four slots, and only then the call it was asked
//! for. Where the whole program leaves `Function` as the language defines it —
//! the `only_a_base` proof, which refuses `Function.prototype` reached at all —
//! the compiler emits one of these instead.
//!
//! # What the runtime still checks, and why it is enough
//!
//! The proof covers the PROTOTYPE. It does not cover the receiver, which is any
//! value the program wrote `.call` after: a plain object with a `call` method of
//! its own, a function given an own `call`, or a function whose prototype was
//! replaced through `Object.setPrototypeOf`. So the door asks three things a
//! real function answers from its cell — callable, no prototype of its own, no
//! own `call` — and anything else takes `direct_call::through_the_method`, the
//! call the compiler would have emitted. `inherited_from` is where those three
//! come from: they are exactly the conditions under which it answers
//! `Function.prototype` for a callable, so the door and the property read
//! cannot disagree about which `call` a receiver has.
//!
//! # Why `apply` keeps the member's own body
//!
//! A real array of at most [`ARGUMENT_SLOTS`] elements is the fast path, and it
//! is the row that was measured. Everything else — an array-like, a nullish
//! list, a longer array — is `function_proto::apply_to`, the same function the
//! member runs, so the two spellings cannot disagree about what a list is.

use super::functions::ARGUMENT_SLOTS;
use super::direct_call::through_the_method;
use super::objects::undefined_of;
use super::with_current;
use crate::value::Value;

/// Whether `callee` is a function whose `call`/`apply` can only be the
/// language's: callable, no prototype of its own, no own property `member`.
fn plain_function(callee: u64, member: &str) -> bool {
    with_current(|context| {
        let Some(cell) = Value(callee).as_slot() else {
            return false;
        };
        if context.callable_at(cell).is_none() || context.prototype_at(cell).is_some() {
            return false;
        }
        let key = context.well_known(member);
        super::objects::own_property(context, cell, key).is_none()
    })
}

/// `f.call(thisArg, a0, a1, a2)`: `argc` says how many operands the program
/// WROTE after the callee, the receiver included — the compiler pads the rest
/// with `undefined` — so the callee sees `argc - 1` arguments. `f.call(o)` is a
/// call with none, and the first draft answered one.
#[rtse::entry]
pub fn function_call_direct(
    callee: u64,
    this_arg: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    argc: i64,
    name: i64,
) -> u64 {
    let written = (argc - 1).clamp(0, 3) as usize;
    if !plain_function(callee, "call") {
        let held = [this_arg, a0, a1, a2];
        return through_the_method(callee, "call", name, &held[..1 + written]);
    }
    let absent = with_current(|context| undefined_of(context));
    super::functions::call_counted(callee, this_arg, written as i64, name, a0, a1, a2, absent)
}

/// `f.apply(thisArg, [a0, a1, a2])` — the list written as a literal, so the
/// compiler hands over its elements and nothing is built: the array cost 115
/// of the 195 ns the row took (release, 2026-09-29). `argc` counts the
/// receiver and the elements, as [`function_call_direct`]'s does.
///
/// Where the receiver is not a plain function the list is BUILT, here, and the
/// `apply` it has is called with it: the program wrote `apply`, and a receiver
/// with its own is given exactly that.
#[rtse::entry]
pub fn function_apply_listed_direct(
    callee: u64,
    this_arg: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    argc: i64,
    name: i64,
) -> u64 {
    let written = (argc - 1).clamp(0, 3) as usize;
    if !plain_function(callee, "apply") {
        let held = [a0, a1, a2];
        let list = with_current(|context| super::array::built_in_from(context, &held[..written]));
        return through_the_method(callee, "apply", name, &[this_arg, list]);
    }
    let absent = with_current(|context| undefined_of(context));
    super::functions::call_counted(callee, this_arg, written as i64, name, a0, a1, a2, absent)
}

/// `f.apply(thisArg, list)`.
#[rtse::entry]
pub fn function_apply_direct(callee: u64, this_arg: u64, list: u64, name: i64) -> u64 {
    if !plain_function(callee, "apply") {
        return through_the_method(callee, "apply", name, &[this_arg, list]);
    }
    // A real array of at most four elements is read from its store and passed
    // in the convention's own slots; nothing is built.
    let short = with_current(|context| {
        let cell = Value(list).as_slot()?;
        let elements = context.elements_at(cell)?;
        if elements.len() > ARGUMENT_SLOTS {
            return None;
        }
        let absent = undefined_of(context);
        let mut slots = [absent; ARGUMENT_SLOTS];
        for (slot, value) in slots.iter_mut().zip(elements) {
            *slot = *value;
        }
        Some((slots, elements.len()))
    });
    match short {
        Some((slots, count)) => super::functions::call_counted(
            callee,
            this_arg,
            count as i64,
            name,
            slots[0],
            slots[1],
            slots[2],
            slots[3],
        ),
        None => super::function_proto::apply_to(callee, this_arg, list),
    }
}
