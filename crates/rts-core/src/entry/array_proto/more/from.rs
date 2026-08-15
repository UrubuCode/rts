//! `Array.from(items, mapFn, thisArg)`.
//!
//! # Why it left [`super`]
//!
//! Because it is the only method here that reads something which is **not** an
//! array, and doing that correctly is three separate rules: which values the
//! iteration protocol already answers, `ToLength` of a `length` an object merely
//! claims, and `Get` of every index below it. Those had been written as one
//! function that walked the shape directly, and each rule was wrong in its own
//! way — `{length: "3"}` was refused for not being a number, and an index defined
//! by a getter answered `undefined` without the getter ever running.
//!
//! The reader itself is [`super::super::like`], shared with `concat`, because
//! spreading a `Symbol.isConcatSpreadable` object is the same operation. Two
//! copies is what there were, and they disagreed about both halves.

use super::super::super::objects::undefined_of;
use super::super::super::{Context, throw, with_current};
use super::super::{built, like};
use super::{calls, nothing, snapshot};
use crate::value::Value;

/// `Array.from(items, mapFn, thisArg)`.
///
/// Through the runtime's own iteration, so anything `for-of` walks this accepts
/// — including a string, which becomes one element per code point rather than
/// per unit.
///
/// # Why the array-like fallback is here and not in `iterate`
///
/// `Array.from({length: 2})` is `[undefined, undefined]` and
/// `for (const x of {length: 2})` is a `TypeError`: the language reads indices
/// off a `length` **only for this function**. Putting the fallback in `iterate`
/// would make `for-of` accept what the language refuses, which is a wrong
/// program that runs. So `iterate` still decides what is *iterable*, and this
/// decides what it additionally accepts when nothing was iterated.
///
/// # The divergence, named
///
/// The iterator is walked to exhaustion BEFORE the mapper runs over any of it,
/// so a mapper that throws does not stop `next()` and no `return()` is ever
/// called. That is [`super::super::super::iterate`]'s materialising shape seen
/// from here rather than a decision of this function — it answers an array, and
/// there is no cursor to interleave with. Interleaving is what that module's own
/// documentation names as the lazy form.
pub(in crate::entry) extern "C" fn from(
    _e: u64,
    _this: u64,
    items: u64,
    mapper: u64,
    receiver: u64,
    _a3: u64,
) -> u64 {
    // Outside every borrow: `iterate` is an entry point, and it may run a
    // user-defined `Symbol.iterator` to exhaustion before it answers — and the
    // array-like reader runs a getter per index.
    let produced = match array_like(items) {
        true => built(like::values_of(items)),
        false => super::super::super::iterate::iterate(items),
    };
    if throw::in_flight() {
        return nothing();
    }
    if !calls(mapper) {
        // Already a fresh array — `iterate` copies — so there is nothing to
        // build a second time.
        return produced;
    }
    let Some(elements) = snapshot(produced) else {
        return nothing();
    };
    // ROOTED, and written as a loop rather than a `collect` for that reason:
    // the mapper is user code that allocates, an allocation collects, and what
    // it has already answered would otherwise live only in a `Vec` on the Rust
    // heap, which no scan of ours reaches. Same hole as `map`'s, same mechanism
    // — see `entry::rooted`.
    let mut mapped = super::super::super::rooted::Rooted::new();
    for (index, element) in elements.iter().enumerate() {
        // `visit` is `map`'s call, and it is shared rather than repeated so that
        // the receiver, the argument order and the rule-8 check are decided in
        // one place. `thisArg` is this function's THIRD argument, which used to
        // be read as padding and thrown away.
        let Some(answered) =
            super::super::iterate::visit(mapper, receiver, produced, *element, index)
        else {
            break;
        };
        mapped.values().push(answered);
    }
    built(mapped.take())
}

/// Whether this value is read by its `length` rather than by iterating it.
///
/// Shared with [`super::from_async`] rather than copied, which is what the
/// visibility is for: `Array.from({length: 2})` and `Array.fromAsync({length:
/// 2})` must agree about what an array-like is, and two spellings of this test is
/// where one of them learns about `Symbol.iterator` and the other does not.
///
/// False for everything iteration already answers — an array, a string, a
/// collection, or an object declaring `Symbol.iterator` — so the fallback can
/// never shadow the real protocol. `Symbol.iterator` is asked LAST because it is
/// the only one of the four that costs a chain walk.
///
/// The `length` itself is not read here, and that is the change: it used to be
/// read under this borrow, refused for not already being a number, and then read
/// again per index off the shape. `ToLength` and `Get` are both operations that
/// may run user code, so both moved out to [`like::values_of`] — which means a
/// non-iterable object always takes the array-like path and an absent `length`
/// answers zero elements there, exactly as `ToLength(undefined)` says.
pub(super) fn array_like(items: u64) -> bool {
    with_current(|context| {
        let Some(cell) = Value(items).as_slot() else {
            return false;
        };
        if context.elements_at(cell).is_some()
            || context.text_at(cell).is_some()
            || super::super::super::collections::iterated(context, cell).is_some()
        {
            return false;
        }
        !declares_iterator(context, cell)
    })
}

/// Whether an object declares how to be iterated, anywhere along its chain.
///
/// A data read: a getter on `Symbol.iterator` would be user code and this is
/// inside a borrow — the same boundary [`super::super::super::iterate`] draws
/// for `next` and `done`.
fn declares_iterator(context: &mut Context, cell: u32) -> bool {
    let key = context.well_known(&format!(
        "{}iterator",
        super::super::super::symbol::PREFIX
    ));
    match super::super::super::objects::read_property(context, cell, key) {
        // Present but `undefined` is the language's own way of saying "not
        // iterable after all", and it has to read as absent here or an object
        // that deliberately erased an inherited iterator would be refused
        // rather than read by its `length`.
        Some(found) => found.bits() != undefined_of(context),
        None => false,
    }
}
