//! `Promise.all`, `allSettled`, `race` and `any`.
//!
//! # What the argument can be, and what it cannot
//!
//! An **array**, today. [`super::elements_of`] goes through
//! `iterate`, which answers an array for an array and for a
//! string and an EMPTY array for everything else — so `Promise.all(aSet)`,
//! `Promise.all(aGenerator)` and `Promise.all(m.values())` fulfil immediately
//! with `[]` rather than waiting for anything they contain. That is the
//! iteration protocol's stated gap — no object here can declare
//! `Symbol.iterator` — and not something these four could fix locally: an
//! implementation that special-cased `Set` would be a second answer to what an
//! iterable is.
//!
//! # Why an empty input is decided before anything is allocated
//!
//! Because the four answers differ, and three of them are immediate:
//! `all([])` and `allSettled([])` fulfil with `[]`, `any([])` rejects at once,
//! and `race([])` **stays pending for ever** — which is the specification and
//! not an oversight. Deciding it first also keeps the group machinery from
//! having to represent a count that starts at zero.

use rts_cranelift::sched::Settlement;

use super::group::{Group, Kind};
use super::react::Handler;
use super::state;
use crate::entry::objects::undefined_of;
use crate::entry::with_current;
use crate::value::Value;

/// The shared body of the four static combinators.
pub(super) fn combine(iterable: u64, kind: Kind) -> u64 {
    // Before any borrow: `iterate` is an entry point and takes one of its own.
    let elements = super::elements_of(iterable);
    if elements.is_empty() {
        return empty(kind);
    }
    // The array every collecting kind eventually answers is built through
    // `array_new`, which is an entry point — so it cannot be made from inside
    // the borrow below. It is made when the group finishes, in the drain.
    with_current(|context| {
        let absent = undefined_of(context);
        let Some((cell, result)) = state::fresh(context) else {
            return absent;
        };
        let group = context
            .promises
            .open(Group::new(kind, result, vec![absent; elements.len()]));
        for (index, element) in elements.into_iter().enumerate() {
            let Some(source) = observed(context, element) else {
                // The region is full, so the next allocation fails too and a
                // group short of one element would never reach zero — a promise
                // that hangs, which is the one outcome worse than a rejection.
                let reason = state::type_error(context, "the heap is full");
                state::reject(context, result, reason);
                return Value::from_slot(cell).bits();
            };
            state::react(context, source, Handler::Member { group, index });
        }
        Value::from_slot(cell).bits()
    })
}

/// The promise an element is observed through.
///
/// A promise is observed as it stands; anything else is wrapped in one that is
/// resolved with it — which is `Promise.resolve`'s rule, and reusing it is what
/// makes `Promise.all([1, thenable, p])` treat all three the same way.
fn observed(
    context: &mut crate::entry::Context,
    element: u64,
) -> Option<rts_cranelift::sched::PromiseId> {
    if let Some(cell) = Value(element).as_slot()
        && let Some(id) = context.promises.id_of(cell)
    {
        return Some(id);
    }
    let (_, id) = state::fresh(context)?;
    state::resolve(context, id, element);
    Some(id)
}

/// What each combinator answers for an input with nothing in it.
fn empty(kind: Kind) -> u64 {
    // Built before the borrow, because an array comes from an entry point —
    // `all` and `allSettled` fulfil with it, and `any` reports it as the
    // aggregate's (empty) `errors`.
    let collected = match kind {
        Kind::Race => None,
        _ => Some(super::array_of(Vec::new())),
    };
    with_current(|context| {
        let Some((cell, id)) = state::fresh(context) else {
            return undefined_of(context);
        };
        match (kind, collected) {
            (Kind::All | Kind::AllSettled, Some(array)) => {
                state::settle(context, id, Settlement::Fulfilled, array);
            }
            (Kind::Any, Some(array)) => {
                let reason = super::group::aggregate(context, array);
                state::reject(context, id, reason);
            }
            // `Promise.race([])` never settles. Nothing to do is the answer, not
            // a case that was forgotten.
            (Kind::Race, _) => {}
            // Unreachable: every kind but `race` carries the array above.
            (_, None) => {}
        }
        Value::from_slot(cell).bits()
    })
}
