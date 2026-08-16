//! What a class a HOST builds still owes the language.
//!
//! `#[rtse::class]` gives a class in this crate four things a program reads
//! without ever calling a method: `C.name`, `C.length`, `C.prototype` and
//! `C.prototype.constructor`. A host outside this crate builds its classes at
//! run time through [`super::modules::make_prototype`] and `make_callable`,
//! which give it the prototype and the callable and none of the four — so
//! `new Event("x").constructor` read `undefined`, and a fixture doing the most
//! ordinary thing there is with a class instance died on it rather than
//! disagreeing about a detail.
//!
//! # Why this is a function and not four calls at each site
//!
//! Because it is one fact — "this callable is the class of that prototype" —
//! and the four writes are how it is recorded. Written out at each host class
//! (there are dozens, across `rts-std` and `rts-node`) it is four chances per
//! class to forget one, and the one most likely to be forgotten is the back
//! link: nothing a host itself does needs `constructor`, so its absence shows
//! up only in a program.
//!
//! # Why the attributes are set rather than left ordinary
//!
//! `constructor`, `name` and `length` are all non-enumerable in the language.
//! Left enumerable, `for (const k in event)` walks the chain and answers
//! `constructor` among the event's own fields, and `Object.keys(Event)` answers
//! `["name", "length"]` for a class the language says has no enumerable keys.

use super::Context;
use crate::value::Value;

/// Ties a host's constructor to its prototype, the way a declared class is tied
/// to its own.
///
/// `arity` is `SetFunctionLength`'s number — the parameters the language says
/// the constructor declares, which is not the four slots every native reads.
pub fn declare_host_class(
    context: &mut Context,
    constructor: u64,
    prototype: u64,
    name: &str,
    arity: u32,
) {
    describe_callable(context, constructor, name, arity);
    if let Some(cell) = Value(prototype).as_slot() {
        let key = context.well_known("constructor");
        super::objects::put(context, cell, key, constructor);
        super::native::hidden(context, cell, key);
    }
}

/// `f.name` and `f.length` on a callable a host built.
///
/// The half of [`declare_host_class`] that is not about a class: a static
/// method (`AbortSignal.any`, `AbortSignal.timeout`) is an ordinary function
/// and the language pins both properties on it too. `make_callable` writes
/// neither, so every one of them answered `undefined` for questions a program
/// asks of anything it is handed.
pub fn describe_callable(context: &mut Context, callable: u64, name: &str, arity: u32) {
    super::native::name_of(context, callable, name);
    super::native::length_of(context, callable, arity);
}
