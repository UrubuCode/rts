//! `Function.prototype` — what every callable inherits from.
//!
//! # Why the link is substituted rather than stored
//!
//! For the reason a string's is. `closure_new` runs at every function
//! definition, and writing a prototype link there would spend a word per
//! function to record one fact all of them share — and it would have to be
//! written while the prototype object itself is being built, which is the
//! recursion `string::prototype_of` records paying for once already.
//!
//! So the chain walk asks instead: a callable with no own prototype reaches
//! [`super::objects::inherited_from`], and that is where this object is named.
//! `Object.setPrototypeOf(f, p)` still wins, because the own link is asked for
//! first.
//!
//! # Why `bind` is not here
//!
//! It is the one of the three that has to *keep* something: a bound function
//! remembers a receiver and a list of arguments, which is a table of **values**
//! beside a cell, and the collector cannot see one. `Aside<Regexp>` holds no
//! values and the accessor table holds them invisibly — a live gap this would
//! widen rather than a new one, and `docs/engine/authoring-natives.md` §8 names who
//! owns the collector's view of an `Aside` as the thing to settle first.
//!
//! `call` and `apply` keep nothing. They are reached, they jump, they are done —
//! which is why they land now and `bind` waits for the answer rather than for a
//! session.

use super::{Context, with_current};
use crate::value::Value;

/// `Function`.
#[rtse::class("Function")]
impl Function {
    /// `f.call(thisArg, a, b, c)`.
    ///
    /// Three arguments rather than four, because the receiver takes one of the
    /// convention's slots. A call with more is refused at the site rather than
    /// losing its arguments here — the same trade `Object.assign` makes, and
    /// `apply` is what a program with an unknown number reaches for.
    fn call(this: u64, receiver: u64, a: u64, b: u64, c: u64) -> u64 {
        // Straight through, with no borrow held: the callee is user code, and
        // its first act may be to call the runtime. `super::functions::call`
        // takes and gives back its own.
        let absent = with_current(|context| super::objects::undefined_of(context));
        super::functions::call(this, receiver, a, b, c, absent)
    }

    /// `f.apply(thisArg, args)`.
    ///
    /// The reason the argument vector had to exist first: this is the spelling
    /// that carries any number of arguments, and it is `call_with_args` from the
    /// other side.
    fn apply(this: u64, receiver: u64, arguments: u64) -> u64 {
        super::functions::call_with_args(this, receiver, arguments)
    }
}

/// What every callable inherits from, made once.
///
/// Lazily, like every other built-in prototype here: a program whose functions
/// are only ever called should not spend the cells for two methods it never
/// reads.
pub(super) fn prototype_of(context: &mut Context) -> Option<u32> {
    if super::class_support::made(context, "Function").is_none() {
        register_function(context);
    }
    Value(super::class_support::prototype(context, "Function")?).as_slot()
}
