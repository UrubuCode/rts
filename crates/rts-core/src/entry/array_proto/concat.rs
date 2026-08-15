//! `a.concat(…)`, and the one question it asks about every item.
//!
//! # Why this left [`super`]
//!
//! Because "does this argument spread?" stopped being a property of the RUNTIME's
//! knowledge — "is there an element vector behind this cell" — and became a
//! property of the PROGRAM: `Symbol.isConcatSpreadable` answers it in both
//! directions, so an array can refuse to spread and a plain object can insist on
//! it. That is a lookup, a coercion and an array-like read, which is more than a
//! `match` in the middle of a method that also allocates the answer.

use super::super::rooted::Rooted;
use super::super::string::absent;
use super::super::{throw, with_current};
use super::{arguments_at, built};
use crate::value::Value;

/// `a.concat(…)` — a new array, with everything spreadable spliced in.
///
/// One level deep, which is the language: `[1].concat([[2]])` is `[1, [2]]`. A
/// recursive splice would flatten what the program deliberately nested.
///
/// The receiver is the first ITEM rather than a special case above the loop,
/// which is what the specification says and what makes
/// `Object.assign([1,2], {[Symbol.isConcatSpreadable]: false}).concat(3)` answer
/// `[[1,2], 3]`. Treating it separately is how the receiver comes to obey a rule
/// its own arguments do not.
pub(super) extern "C" fn concat(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let given = with_current(|context| arguments_at(context, 0, [a0, a1, a2, a3]));
    // ROOTED: reading a spreadable object's indices runs getters, which is user
    // code that allocates — and what has been joined so far would otherwise be
    // named only by a `Vec` the collector cannot reach. See `super::super::rooted`.
    let mut joined = Rooted::new();
    for item in std::iter::once(this).chain(given) {
        match spread(item) {
            Some(values) => joined.values().extend(values),
            None => joined.values().push(item),
        }
        // A getter that threw stops the concatenation, rather than letting the
        // remaining items be appended around a hole in the middle of it.
        if throw::in_flight() {
            break;
        }
    }
    built(joined.take())
}

/// What an item contributes when it spreads, or `None` when it goes in whole.
///
/// `IsConcatSpreadable` in both directions, which is the whole point of the
/// symbol: an array carrying `false` is one element, and an object carrying
/// `true` is read as an array-like even though nothing else here would call it
/// one.
fn spread(item: u64) -> Option<Vec<u64>> {
    let decided = with_current(|context| {
        let cell = Value(item).as_slot()?;
        // A data read. A getter on `Symbol.isConcatSpreadable` would be user
        // code, and this is inside the borrow — the same boundary
        // `super::super::iterate` draws for `next` and `done`, and named for the
        // same reason: an accessor on this key is not something a real program
        // writes, while an accessor on an INDEX is, which is why the index reads
        // below happen outside every borrow instead.
        let key = context.well_known(&format!(
            "{}isConcatSpreadable",
            super::super::symbol::PREFIX
        ));
        let flag = super::super::objects::read_property(context, cell, key).map(|held| held.bits());
        let held = context.elements_at(cell).is_some();
        let spreads = match flag {
            Some(flag) if !absent(context, flag) => {
                super::super::primitives::to_boolean_in(context, flag)
            }
            // Absent — so `IsArray` decides, which is the ordinary case and the
            // one every program that never names the symbol takes.
            _ => held,
        };
        match (spreads, held) {
            (false, _) => None,
            // An array's elements are already here, so nothing about the
            // array-like protocol is worth running over one: it would read the
            // same words through a property lookup per index.
            (true, true) => Some(Spread::Elements(context.elements_at(cell)?.clone())),
            (true, false) => Some(Spread::Like),
        }
    })?;
    match decided {
        Spread::Elements(elements) => Some(elements),
        // Outside the borrow above, because every index read is a `Get`.
        Spread::Like => Some(super::like::values_of(item)),
    }
}

/// What an item turned out to be, decided under a borrow that then ends.
enum Spread {
    /// An array, whose elements were read while the decision was made.
    Elements(Vec<u64>),
    /// Something that claims a `length`, which still has to be read by index.
    Like,
}
