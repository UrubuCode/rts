//! The four methods that grow or shrink an array from one of its ends.
//!
//! # Why they are a module of their own
//!
//! Because they are the four whose generic arm WRITES: `push` and `unshift`
//! `Set` positions and `length` on whatever the receiver is, `pop` and `shift`
//! also `DeletePropertyOrThrow`. The dense arm is in place in the element
//! vector; the generic one is [`super::generic`]. Moved out of `mod.rs`, which
//! had passed the crate's 500-line ceiling, when the generic arms arrived.

use super::super::objects::undefined_of;
use super::super::{Context, with_current};
use super::{arguments, arguments_at, generic, staged, store};
use crate::value::Value;

/// `a.push(…)` — answers the new length, or throws when the array refuses one.
///
/// # Why the refusal is a throw and why it is decided BEFORE the append
///
/// `push` is defined as `Set(O, ToString(len), E, true)` followed by
/// `Set(O, "length", len, true)`, and an array's `[[DefineOwnProperty]]`
/// rejects an index at or past a `length` that is not writable — so the throw
/// happens on the first element and nothing is stored. Appending first and
/// letting `set_length` quietly fail is what this used to do, and it produced
/// an array disagreeing with itself: `Object.defineProperty(a, "length",
/// {writable: false}); a.push(4)` left `a.length` at 3 with four elements in
/// it, which every read of `a[3]` could see and every loop over `a.length`
/// could not.
///
/// The raise is OUTSIDE the borrow. `throw::type_error` builds the program's
/// own `TypeError`, which takes the context — raising from inside would
/// re-enter the `RefCell`, and an `extern "C"` frame cannot unwind out of that,
/// so it ends the process rather than the call. The message is therefore built
/// in and thrown out, which is the two-stage shape every native that raises
/// here uses.
pub(super) extern "C" fn push(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let (answer, refused) = with_current(|context| {
        // Into a buffer on THIS frame, and a `Vec` only when the call spilled
        // past the four slots. `a.push(i)` carries one word whose count the site
        // already declared and which is already in a register, so allocating a
        // heap vector to hold it is paying for a question that was answered
        // while compiling.
        //
        // Copied out rather than borrowed because `elements_at_mut` below takes
        // a mutable borrow of the same context a spilled slice would come from —
        // which is why the buffer is what makes this work at all, rather than
        // merely what makes it fast.
        let carried = arguments::arguments_owned_at(context, 0, [a0, a1, a2, a3]);
        let more = carried.as_slice();
        // Not an array: the generic arm, outside this borrow.
        let Some(cell) = Value(this).as_slot().filter(|cell| context.elements_at(*cell).is_some())
        else {
            return (None, Some(Err(carried.as_slice().to_vec())));
        };
        // Nothing to add is nothing to refuse: `a.push()` re-states the length
        // it already has, and re-stating the same value is permitted even on a
        // non-writable property.
        if !more.is_empty() && refuses_append(context, cell) {
            return (Some(undefined_of(context)), Some(Ok(refusal(context, cell))));
        }
        // Appended IN PLACE. `staged` copies, and it copies for a reason —
        // a method that calls user code cannot hold a borrow of the context
        // across the call — but `push` calls nothing. Copying here made
        // building an array O(N^2): `Vec::clone` allocates capacity exactly
        // equal to length, so the `extend` that follows reallocated every
        // time. Two allocations and two O(n) copies per element appended.
        //
        // The borrow ends before `set_length`, which is why this is two
        // statements and not one.
        let Some(elements) = context.elements_at_mut(cell) else {
            return (Some(undefined_of(context)), None);
        };
        elements.extend_from_slice(&more);
        let count = elements.len();
        super::super::array::set_length(context, cell, count);
        (Some(Value::from_f64(count as f64).bits()), None)
    });
    match (answer, refused) {
        (_, Some(Err(values))) => {
            // ROOTED: the generic arm runs setters and traps, which allocate.
            let values = super::super::rooted::Rooted::with(values);
            match generic::object(this, "push") {
                Some(object) => generic::push(object, values.as_slice()),
                None => nothing(),
            }
        }
        (answer, Some(Ok(message))) => {
            super::super::throw::type_error(&message);
            answer.unwrap_or_else(nothing)
        }
        (answer, None) => answer.unwrap_or_else(nothing),
    }
}

/// The `undefined` a method answers when there is nothing to answer.
fn nothing() -> u64 {
    with_current(|context| undefined_of(context))
}

/// Whether an array refuses to grow.
///
/// Asked of `length` rather than of the elements, because that is where the
/// language records it: `Object.defineProperty(a, "length", {writable: false})`
/// and `Object.freeze(a)` are the two ways to reach this, and
/// [`super::super::integrity::refuses_key_write`] already folds the object's own
/// refusal into the property's — so one question answers both instead of two
/// that could come to disagree.
fn refuses_append(context: &mut Context, cell: u32) -> bool {
    match super::super::computed::length_key(context) {
        crate::object::Key::Name(named) => {
            super::super::integrity::refuses_key_write(context, cell, named)
        }
        // `length` is a name, always. A key registry that answered otherwise is
        // malformed, and refusing every push over it would be a wrong answer
        // dressed as caution.
        crate::object::Key::Index(_) => false,
    }
}

/// What the refusal SAYS, which differs by which of the two caused it.
///
/// The message is the only part of a `TypeError` a program usually reads, and
/// the two causes are genuinely different repairs: a frozen array needs the
/// freeze removed and an array with a pinned `length` needs the descriptor
/// changed. One message for both would name the wrong one half the time.
fn refusal(context: &Context, cell: u32) -> String {
    if super::super::integrity::refuses_write(context, cell) {
        let at = context.elements_at(cell).map_or(0, Vec::len);
        return format!("Cannot add property {at}, object is not extensible");
    }
    "Cannot assign to read only property 'length' of object '[object Array]'".to_owned()
}

/// `a.pop()` — the last element, removed.
pub(super) extern "C" fn pop(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let (answer, refused) = with_current(|context| {
        let Some(cell) = Value(this).as_slot().filter(|cell| context.elements_at(*cell).is_some())
        else {
            return (None, None);
        };
        // An empty array answers `undefined` and stays empty. Re-stating zero
        // is permitted even when `length` is non-writable; a non-empty array,
        // however, must be rejected before its last element is removed.
        if context
            .elements_at(cell)
            .is_some_and(|elements| !elements.is_empty())
            && refuses_append(context, cell)
        {
            return (Some(undefined_of(context)), Some(refusal(context, cell)));
        }
        // `visible`: um buraco no fim sai como `undefined`, não como o
        // marcador — este é um dos quatro pontos que devolvem o word CRU ao
        // programa sem passar por `get_indexed`.
        // Removed in place, for the reason `push` appends in place: nothing
        // here calls user code, so nothing needs the copy.
        let taken = match context.elements_at_mut(cell) {
            Some(elements) => elements.pop(),
            None => return (Some(undefined_of(context)), None),
        };
        let count = context.elements_at(cell).map_or(0, Vec::len);
        super::super::array::set_length(context, cell, count);
        let taken = taken.unwrap_or_else(|| undefined_of(context));
        (Some(super::super::array::visible(context, taken)), None)
    });
    if let Some(message) = refused {
        super::super::throw::type_error(&message);
    }
    match answer {
        Some(answer) => answer,
        None => generic::object(this, "pop").map_or_else(nothing, generic::pop),
    }
}

/// `a.shift()` — the first element, removed.
pub(super) extern "C" fn shift(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let dense = with_current(|context| {
        let (cell, mut elements) = staged(context, this)?;
        if elements.is_empty() {
            return Some(undefined_of(context));
        }
        let taken = super::super::array::visible(context, elements.remove(0));
        store(context, cell, elements);
        Some(taken)
    });
    match dense {
        Some(answer) => answer,
        None => generic::object(this, "shift").map_or_else(nothing, generic::shift),
    }
}

/// `a.unshift(…)` — answers the new length.
pub(super) extern "C" fn unshift(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let dense = with_current(|context| {
        let more = arguments_at(context, 0, [a0, a1, a2, a3]);
        let Some((cell, elements)) = staged(context, this) else {
            return Err(more);
        };
        // The arguments keep their order at the front, which a loop of
        // `insert(0, …)` would reverse — the corner that makes
        // `[3].unshift(1, 2)` produce `[2, 1, 3]`.
        let mut joined = more;
        joined.extend_from_slice(&elements);
        let count = joined.len();
        store(context, cell, joined);
        Ok(Value::from_f64(count as f64).bits())
    });
    match dense {
        Ok(answer) => answer,
        Err(values) => {
            let values = super::super::rooted::Rooted::with(values);
            match generic::object(this, "unshift") {
                Some(object) => generic::unshift(object, values.as_slice()),
                None => nothing(),
            }
        }
    }
}
