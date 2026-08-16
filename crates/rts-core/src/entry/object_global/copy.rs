//! `Object.assign` — copying own enumerable properties from one object to
//! another.
//!
//! Its own module because [`super`] is at the ceiling this workspace sets, and
//! because this is the half of `Object` that COPIES rather than describes:
//! `{ ...source }` is the same operation under different syntax, and
//! `super::super::objects::object_spread` is where that spelling lives. The two
//! agreeing about what "own enumerable" means — symbols included — is the whole
//! reason the symbol pass below reads the way that one does.

use super::super::{Context, with_current};
use crate::value::Value;

/// An array's elements, the borrow ending here.
///
/// Shared with [`super`], whose `entries` and `fromEntries` read a list the same
/// way. One spelling rather than three, because a second would be a second
/// answer to what an array that is not one answers.
pub(super) fn held(array: u64) -> Option<Vec<u64>> {
    with_current(|context: &mut Context| {
        let cell = Value(array).as_slot()?;
        Some(context.elements_at(cell)?.clone())
    })
}

/// The keys `EnumerableOwnProperties` visits, with the descriptor asked
/// **per key** and immediately before that key's read.
///
/// # Why the order and not just the set
///
/// `Object.values`, `Object.entries`, `Object.assign` and object spread are all
/// defined as one loop: `[[OwnPropertyKeys]]` once, then `[[GetOwnProperty]]`
/// and `[[Get]]` alternating per key. Over a plain object nothing can tell,
/// which is why every one of them here asked `super::super::array::own_keys` —
/// the already-filtered list — and then read the values in a second pass.
///
/// A **proxy** can tell, and a program that logs its handler is exactly what
/// these fixtures do: `super::super::proxy::enumerable_keys` runs every
/// `getOwnPropertyDescriptor` first, so the sequence came out
/// `ownKeys, gopd:a, gopd:b, get:a, get:b` where every other engine reports
/// `ownKeys, gopd:a, get:a, gopd:b, get:b`. Same answer, different observable
/// order — and the order is the thing a trap exists to observe.
///
/// The filter is skipped entirely for a non-proxy, because `own_keys` has
/// already applied it there and asking again would be a second descriptor read
/// per property on the path every ordinary `{...o}` takes.
/// A closure and not a returned list, because a list is what forces the wrong
/// order: the caller's work for key *a* has to happen before the descriptor of
/// key *b* is asked for.
pub(in crate::entry) fn each_enumerable_own(object: u64, mut visit: impl FnMut(u64)) {
    let filter = super::super::proxy::is_proxy(object);
    let listed = match filter {
        true => super::super::proxy::own_keys(object).unwrap_or(object),
        false => super::super::array::own_keys(object),
    };
    if super::super::throw::in_flight() {
        return;
    }
    let Some(names) = held(listed) else {
        return;
    };
    for name in names {
        if filter {
            let described = super::describe_of(object, name);
            // Rule 8: the descriptor came from a handler, so it may not have
            // come at all.
            if super::super::throw::in_flight() {
                return;
            }
            let enumerable = with_current(|context| {
                let key = context.well_known("enumerable");
                Value(described)
                    .as_slot()
                    .and_then(|cell| super::super::objects::read_property(context, cell, key))
                    .map(|found| found.bits())
            });
            if !enumerable.is_some_and(|found| super::super::primitives::to_boolean(found)) {
                continue;
            }
        }
        visit(name);
        if super::super::throw::in_flight() {
            return;
        }
    }
}

/// `Object.assign(target, ...sources)` — copies the own keys across.
///
/// Three sources rather than any number, because the arity a call carries is
/// four and the receiver takes one of them. It used to read ONE and the comment
/// beside it claimed a call with more was "refused at the site"; it was not —
/// `Object.assign({}, a, b)` silently dropped `b`, which is the failure mode a
/// merge must not have, because the result looks like a merge.
///
/// Beyond three the arguments genuinely are not here, and that stays a gap
/// rather than a quiet loss: it needs the gathered-arguments path, which is a
/// change to how a native is called and not to this function.
pub(super) extern "C" fn assign(
    _e: u64,
    _this: u64,
    target: u64,
    source: u64,
    a2: u64,
    a3: u64,
) -> u64 {
    for source in [source, a2, a3] {
        // An absent argument arrives as `undefined`, and `undefined` has no own
        // keys — so the same skip serves both the spec's rule (a null or
        // undefined source contributes nothing) and the missing-argument case,
        // without this having to know which it is looking at.
        each_enumerable_own(source, |name| {
            // Through the ordinary paths on both sides, so a getter on the
            // source runs and a setter on the target runs — which is what
            // `assign` does and what a slot-to-slot copy would have skipped.
            let value = super::super::computed::get_indexed(source, name);
            super::super::computed::set_indexed(target, name, value);
        });
        // `own_keys` never reports a symbol — it is the string enumeration,
        // and a symbol has no string spelling for it to report — so a
        // `[sym]: v` entry used to vanish across `Object.assign` and spread
        // silently, which is a merge that drops data rather than one that
        // says it cannot copy something.
        let symbols = with_current(|context| super::super::array::symbol_keyed(context, source));
        for (key, value) in symbols {
            with_current(|context| {
                if let Some(cell) = Value(target).as_slot() {
                    super::super::objects::put(context, cell, key, value);
                }
            });
        }
    }
    target
}
