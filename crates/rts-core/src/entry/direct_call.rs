//! The generic half of a method the compiler reached directly.
//!
//! `m.get(k)` is compiled as one entry point (`collections::direct`) when the
//! whole program leaves `Map` alone: the entry checks the receiver's BRAND and,
//! for a real `Map`, answers from the table with the property read and the
//! dispatch removed. For anything else — an object that happens to have a `get`
//! method, a proxy, `undefined` — the language's answer is whatever calling
//! `this.get(k)` would have been, and this is what produces it: the property
//! read by its key, the same door a compiled call takes, the count the site
//! wrote, and the spelling the site had for its `TypeError`.
//!
//! # What differs from the call the compiler would have emitted, and is accepted
//!
//! The order. A compiled `o.get(k)` reads `o.get` BEFORE evaluating `k`; here
//! the arguments were evaluated first and the read happens now. That is
//! observable only where reading the property has an effect — a getter named
//! `get`, a proxy trap — and only in the order of that effect against the
//! arguments' own. `emit/math` accepted the same for `Math.floor(f())`, for the
//! same reason: the proof that admits the direct form is that the name means
//! the language's own, whose property reads have no effects.

use super::{Context, with_current};

/// `this.<member>(args...)` as the program would have called it.
pub(in crate::entry) fn through_the_method(this: u64, member: &str, name: i64, args: &[u64]) -> u64 {
    let key = with_current(|context| key_number(context, member));
    let callee = super::objects::get_property(this, key);
    if super::throw::in_flight() {
        return with_current(|context| super::objects::undefined_of(context));
    }
    let absent = with_current(|context| super::objects::undefined_of(context));
    let mut slots = [absent; super::functions::ARGUMENT_SLOTS];
    for (slot, value) in slots.iter_mut().zip(args) {
        *slot = *value;
    }
    super::functions::call_counted(
        callee,
        this,
        args.len() as i64,
        name,
        slots[0],
        slots[1],
        slots[2],
        slots[3],
    )
}

/// The machine's number for a well-known member name.
fn key_number(context: &mut Context, member: &str) -> i64 {
    match context.well_known(member) {
        crate::object::Key::Name(named) => named.index() as i64,
        // A member name is never an index; `well_known` interns text, and text
        // that spells an index does not name a method this file is asked for.
        crate::object::Key::Index(index) => i64::from(index),
    }
}
