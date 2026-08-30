//! Which class a method that MAKES a new array puts its answer in.
//!
//! # Why the answer is a question at all
//!
//! `map` and `filter` do not merely produce elements — they produce an OBJECT,
//! and the language lets the receiver decide what kind. The protocol is the
//! receiver's `constructor`, then that constructor's `Symbol.species`, and what
//! comes back is what gets constructed. So `class Fancy extends Array {}` has
//! `fancy.filter(p) instanceof Fancy`, and a subclass that wants plain arrays
//! back says so with `static get [Symbol.species]() { return Array; }`. Both are
//! observable, and both answered the same plain array before this existed.
//!
//! # Why the ordinary case is not routed through it
//!
//! Because the protocol is two property reads and a constructor call, all three
//! of which are user code, and almost every program's answer is "a plain array".
//! So an absent or built-in species takes the path these methods always took —
//! one `array_new` and one store — and the protocol only decides anything for a
//! program that asked it to. [`made`] answering `None` is that case, and it
//! deliberately folds in the three ways of reaching it: a receiver that is not
//! an array, a `constructor` or species that is absent, and a species that IS
//! the built-in `Array`.
//!
//! # What is NOT routed through it yet, said rather than left to be found
//!
//! `slice`, `splice`, `concat` and `flat` consult it now, through
//! [`collected`], which is [`made`] and [`placed`] in one call — the gap this
//! paragraph used to name, closed once rather than written out four times in
//! front of four different loops. `flatMap` still does not: its flattening runs
//! inside a borrow this call could not be taken from, which is its own change.
//! `toSorted`, `toReversed`, `toSpliced` and `with` genuinely never consult it —
//! ES2023 defines all four as always producing a plain `Array`.
//!
//! # What this shares with the typed-array side
//!
//! [`property`] is the same two-statement read `buffers::typed_species` used to
//! keep privately, and that copy is gone rather than left beside this one: a
//! species getter has to run OUTSIDE the borrow that found it, and two spellings
//! of that rule is one of them eventually running inside. What is NOT shared is
//! the rule around it, because the two genuinely differ — an absent
//! `Symbol.species` on a typed array means the receiver's OWN class, where here
//! it means `Array`, and a typed array reaches the protocol from `slice` where
//! an array reaches it from `map`.
//!
//! This module is the shared home only because `array_proto` and `buffers` are
//! siblings and one of them had to hold it. `entry::species`, beside neither, is
//! where it belongs.

use super::super::accessor::Found;
use super::super::{Context, functions, objects, with_current};
use crate::value::Value;

/// `ArraySpeciesCreate(original, length)` — where a method puts what it makes.
///
/// `None` means the plain array the caller would have built anyway, which is
/// every program that did not name a species. See the module documentation for
/// why that is one answer and not four.
pub(super) fn made(original: u64, length: usize) -> Option<u64> {
    // Only a genuine array consults the protocol. The specification checks
    // `IsArray` first for the same reason `Array.isArray` exists: this is the
    // side table, not the prototype, so an array whose prototype was replaced
    // still answers here and a plain object that inherited from one does not.
    let held = with_current(|context| {
        let Some(cell) = Value(original).as_slot() else {
            return false;
        };
        if context.elements_at(cell).is_none() {
            return false;
        }
        true
    });
    if !held {
        return None;
    }
    // Outside every borrow: `constructor` may be a getter and the species is one
    // by construction — `static get [Symbol.species]()` is how it is written.
    let constructor = property(original, "constructor")?;
    // `"@@species"` is the key text a `static get [Symbol.species]()` member
    // writes to — see `entry::symbol` for why a symbol key is a name in a space
    // no program can spell. Reached by that spelling rather than by minting the
    // symbol and asking for its key, because minting takes a borrow to answer
    // what this string already says.
    //
    // An absent species is the CONSTRUCTOR itself, which is what the getter the
    // language puts on `Array` returns: `class Same extends Array {}` maps into
    // a `Same`.
    let answered = property(constructor, super::super::symbol::SPECIES).unwrap_or(constructor);
    let chosen = with_current(|context| {
        // `undefined` and `null` are the two the specification names as "use the
        // default", and a species that cannot be constructed with is a
        // `TypeError` there — answered here as the default too, for the reason
        // every refusal in this folder settles on: a wrong CLASS is a smaller
        // divergence than a process that stops where the language does not.
        if answered == objects::undefined_of(context) {
            return None;
        }
        let Some(cell) = Value(answered).as_slot() else {
            return None;
        };
        if context.callable_at(cell).is_none() {
            return None;
        }
        // The built-in `Array` answers what the default already makes, so it is
        // not worth a construction — and running one would only be slower.
        let builtin = built_in(context) == Some(answered);
        if builtin {
            return None;
        }
        Some(answered)
    });
    let chosen = chosen?;
    let absent = with_current(|context| objects::undefined_of(context));
    let counted = Value::from_f64(length as f64).bits();
    Some(functions::construct(chosen, counted, absent, absent, absent))
}

/// Writes one element into a species-constructed destination.
///
/// Through `set_indexed` rather than the element vector, because the species may
/// have answered anything a constructor can produce — the poison case in
/// `codex_array_species_constructor_poison.ts` answers a plain function whose
/// result is an ordinary array with a property hung on it. A store that assumed
/// an element vector would be a store into whatever came back, which is not a
/// copy but a guess.
pub(super) fn placed(destination: u64, index: usize, value: u64) {
    let at = Value::from_f64(index as f64).bits();
    super::super::computed::set_indexed(destination, at, value, 0 /* strict: um native que escreve reporta a recusa */);
}

/// The `Array` every array inherits its `constructor` from.
///
/// Read off `Array.prototype` rather than remembered separately, because that
/// property IS the thing the protocol above consults: a second record of which
/// callable is the built-in would be a second answer to one question, and the
/// one that disagreed would silently construct where the default was meant.
fn built_in(context: &mut Context) -> Option<u64> {
    let cell = context.array_prototype?;
    let key = context.well_known("constructor");
    objects::read_property(context, cell, key).map(|found| found.bits())
}

/// One property, along the prototype chain, with a getter run if that is what
/// is there.
///
/// Two statements rather than one for the reason `objects::get_property`
/// records: the answer may be a **function to call**, and calling it inside the
/// borrow that found it re-enters the `RefCell`.
///
/// `None` for absent, so that a missing `constructor` and a missing species are
/// one answer at the call site — both mean "the program named nothing here".
pub(in crate::entry) fn property(value: u64, name: &str) -> Option<u64> {
    let found = with_current(|context| {
        let cell = Value(value).as_slot()?;
        let key = context.well_known(name);
        Some(super::super::accessor::resolve(context, cell, key))
    })?;
    match found {
        Found::Value(held) => Some(held),
        Found::Getter(getter) => {
            let absent = with_current(|context| objects::undefined_of(context));
            // The receiver is the object the read was written on: a species
            // getter defined on a subclass reads `this` as that subclass.
            Some(functions::call(
                getter, value, absent, absent, absent, absent,
            ))
        }
        Found::Absent => None,
    }
}


/// The array a method that PRODUCES one answers, species consulted.
///
/// [`made`] plus [`placed`] in one call, because the five callers that were
/// missing the protocol — `slice`, `splice`, `concat`, `flat` and `flatMap` —
/// each had the same three lines to write in front of a different loop, and
/// three lines written five times is where the fourth one is written
/// differently. The module documentation named those five as a gap rather than
/// a decision; this is the gap closed.
///
/// `None` from [`made`] takes the path they always took: one `array_new` and
/// one store, so a program that never named a species pays two property reads
/// and nothing else.
pub(super) fn collected(original: u64, values: Vec<u64>) -> u64 {
    // ROOTED across the construction: the species is user code and allocates,
    // and until the destination holds them these values are named only by a
    // `Vec` on the Rust heap. See `super::super::rooted`.
    let values = crate::entry::rooted::Rooted::with(values);
    let Some(destination) = made(original, values.len()) else {
        return super::built(values.take());
    };
    let values = values.take();
    for (index, value) in values.into_iter().enumerate() {
        placed(destination, index, value);
        if super::super::throw::in_flight() {
            return destination;
        }
    }
    destination
}
