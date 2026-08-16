//! What a combinator is while it is still waiting.
//!
//! # Why a group and not four implementations
//!
//! `all`, `allSettled`, `race` and `any` differ in two decisions and nothing
//! else: which settlement finishes the whole thing immediately, and what is kept
//! from the ones that do not. Written separately they would be four copies of
//! "count down, fill a slot, answer when the count reaches zero", and the
//! counting is the part with an off-by-one in it.
//!
//! # Why the count is not `values.len()`
//!
//! Because a slot is filled in whatever order the promises settle, and an
//! element may legitimately be filled with `undefined`. A count of the slots
//! still outstanding is the only thing that does not have to distinguish "not
//! yet" from "settled with nothing".

use rts_cranelift::sched::{PromiseId, Settlement};

use crate::entry::Context;
use crate::entry::objects::undefined_of;
use crate::text::Str;
use crate::value::Value;

use super::react::Step;

/// Which combinator a group belongs to.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    /// `Promise.all` — every fulfilment, or the first rejection.
    All,
    /// `Promise.allSettled` — a record for each, and never a rejection.
    AllSettled,
    /// `Promise.race` — the first settlement of either kind.
    Race,
    /// `Promise.any` — the first fulfilment, or an aggregate of every rejection.
    Any,
}

/// A combinator in flight.
pub(super) struct Group {
    /// Which one it is.
    pub(super) kind: Kind,
    /// The promise it answers.
    pub(super) result: PromiseId,
    /// What each element contributed, in the order the input had them.
    ///
    /// Input order rather than settlement order, which is the whole reason
    /// `Promise.all` is used instead of a loop: the answer must line up with the
    /// array that went in, however the timing came out.
    pub(super) values: Vec<u64>,
    /// How many elements have not settled yet.
    pub(super) remaining: usize,
}

impl Group {
    /// A group over this many elements, none of them settled.
    pub(super) fn new(kind: Kind, result: PromiseId, values: Vec<u64>) -> Self {
        Group {
            kind,
            result,
            remaining: values.len(),
            values,
        }
    }
}

/// One element of a combinator settled: what that does to the group.
///
/// `None` means the group is not finished — which is the common answer, and the
/// reason this is written as an `Option<Step>` rather than a `bool` and a second
/// call to work out what to do about it.
pub(super) fn advance(
    context: &mut Context,
    at: usize,
    index: usize,
    settlement: Settlement,
    value: u64,
) -> Option<Step> {
    // Read out of the group and let go of it, because two of the four arms below
    // build an object, which needs the context the group is borrowed from.
    let (kind, result) = {
        let group = context.promises.group_mut(at)?;
        (group.kind, group.result)
    };
    match (kind, settlement) {
        // The first settlement wins, either way. A later one reaches
        // `PromiseTable::settle`, which drops it — the same rule the machine
        // applies everywhere, rather than a second guard here.
        (Kind::Race, _) => Some(Step::Finish {
            promise: result,
            settlement,
            value,
        }),
        // One rejection ends `all`, and one fulfilment ends `any`. The values
        // collected so far are abandoned rather than cleared: the group is only
        // ever read through a reaction, and the result promise is settled, so
        // every remaining reaction finds a settled promise and its step is
        // dropped.
        (Kind::All, Settlement::Rejected) | (Kind::Any, Settlement::Fulfilled) => {
            Some(Step::Finish {
                promise: result,
                settlement,
                value,
            })
        }
        (Kind::All, Settlement::Fulfilled) => filled(context, at, index, value).map(|values| {
            Step::Collect {
                promise: result,
                settlement: Settlement::Fulfilled,
                values,
            }
        }),
        (Kind::Any, Settlement::Rejected) => filled(context, at, index, value)
            .map(|reasons| Step::Aggregate {
                promise: result,
                reasons,
            }),
        (Kind::AllSettled, _) => {
            let record = status(context, settlement, value);
            filled(context, at, index, record).map(|values| Step::Collect {
                promise: result,
                settlement: Settlement::Fulfilled,
                values,
            })
        }
    }
}

/// Fills one slot, and answers the values when that was the last one.
fn filled(context: &mut Context, at: usize, index: usize, value: u64) -> Option<Vec<u64>> {
    let group = context.promises.group_mut(at)?;
    let slot = group.values.get_mut(index)?;
    *slot = value;
    group.remaining = group.remaining.saturating_sub(1);
    match group.remaining {
        0 => Some(group.values.clone()),
        _ => None,
    }
}

/// `{ status: "fulfilled", value }` or `{ status: "rejected", reason }`.
///
/// An ordinary object with two data properties, built without running anything —
/// which is what lets it be made here, inside the borrow that decided the step.
pub(super) fn status(context: &mut Context, settlement: Settlement, value: u64) -> u64 {
    let Some(cell) = super::super::native::plain(context) else {
        return undefined_of(context);
    };
    let (state, field) = match settlement {
        Settlement::Fulfilled => ("fulfilled", "value"),
        Settlement::Rejected => ("rejected", "reason"),
    };
    let text = context.intern_value(Str::from_str(state)).bits();
    let key = context.well_known("status");
    super::super::objects::put(context, cell, key, text);
    let key = context.well_known(field);
    super::super::objects::put(context, cell, key, value);
    Value::from_slot(cell).bits()
}

/// The error `Promise.any` rejects with when every input rejected.
///
/// # Why the prototype is `AggregateError.prototype` and not `Error.prototype`
///
/// This used to hang the object off `Error.prototype` and write an own `name` of
/// `"AggregateError"`, because the comment here said there was no such class.
/// There is — `entry::error` declares it — and the two are not the same object
/// to a program: `e instanceof AggregateError` was false, and `e.constructor
/// .name` answered `"Error"`, which is what a `catch` block written against the
/// language reads to decide what it caught.
///
/// The own `name` goes with the change rather than staying beside it: the
/// prototype's `name` already answers `"AggregateError"`, and writing it again
/// on every instance would be the same fact in two places — the one that goes
/// stale is whichever a later edit forgets.
///
/// The constructor is deliberately NOT called. `AggregateError::build` walks its
/// first argument as an iterable, which is user code, and this runs inside the
/// borrow that decided the step — the rule this whole module is written around.
/// The reasons are already a list; there is nothing to iterate.
pub(super) fn aggregate(context: &mut Context, reasons: u64) -> u64 {
    let Some(cell) = super::super::native::plain(context) else {
        return undefined_of(context);
    };
    super::super::error::register_aggregate_error(context);
    if let Some(prototype) = super::super::class_support::prototype(context, "AggregateError") {
        context.set_prototype(cell, prototype);
    }
    let value = context
        .intern_value(Str::from_str("All promises were rejected"))
        .bits();
    let key = context.well_known("message");
    super::super::objects::put(context, cell, key, value);
    let key = context.well_known("errors");
    super::super::objects::put(context, cell, key, reasons);
    // Non-enumerable, which is what `AggregateError`'s own definition says about
    // `errors` and what `message` already gets from the `Error` constructor:
    // left enumerable, `JSON.stringify(err)` and `Object.keys(err)` answered a
    // field no other error has.
    super::super::native::hidden(context, cell, key);
    Value::from_slot(cell).bits()
}
