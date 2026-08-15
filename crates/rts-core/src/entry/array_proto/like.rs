//! Reading an object that merely CLAIMS to be an array.
//!
//! # Why two methods share one reader
//!
//! `Array.from({length: 2})` and `[1].concat(spreadableObject)` are the same
//! operation — `ToLength` of a `length`, then `Get` of every index below it — and
//! they are the only two places in the language that perform it. They had two
//! implementations here and the two disagreed about both halves: `from` read
//! `length` without `ToLength`, so `{length: "3"}` was refused, and `concat`
//! never asked at all, so a spreadable object was appended as one element.
//!
//! # Why the reads go through `get_indexed`
//!
//! Because the specification says `Get`, and `Get` runs a getter and asks a
//! proxy. The previous reader walked the shape directly, so
//! `Object.defineProperty(o, "1", {get() {…}})` answered `undefined` and the
//! getter was never called — a silent wrong value where the program had written
//! the only line that could produce the right one.
//!
//! That is what makes every read here a call into user code, and therefore why
//! nothing in this file may run inside `with_current`: the borrow the entry point
//! would be holding is the one the getter's first runtime call re-enters.

use super::super::rooted::Rooted;
use super::super::{throw, with_current};
use crate::text::Str;
use crate::value::Value;

/// Every element an array-like claims to have, read as `o[i]`.
///
/// Stops on a throw rather than carrying on with the `undefined` a failed call
/// answers — rule 8 of this crate's README, and the reason it applies to a
/// *read*: a getter is user code like any callback, so a `length` getter that
/// threw would otherwise be followed by however many index reads its own number
/// asked for.
///
/// ROOTED, because each read may allocate and an allocation collects: what has
/// already been read lives in a `Vec` on the Rust heap, which no scan of ours
/// reaches. Same hole `map` and `Array.from`'s mapper have, same mechanism —
/// see [`super::super::rooted`].
pub(super) fn values_of(items: u64) -> Vec<u64> {
    let key = with_current(|context| context.intern_value(Str::from_str("length")).bits());
    let claimed = super::super::computed::get_indexed(items, key);
    if throw::in_flight() {
        return Vec::new();
    }
    let count = super::numeric::length(claimed);
    let mut values = Rooted::new();
    for at in 0..count {
        let read = super::super::computed::get_indexed(items, Value::from_f64(at as f64).bits());
        if throw::in_flight() {
            break;
        }
        values.values().push(read);
    }
    values.take()
}
