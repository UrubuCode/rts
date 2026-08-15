//! What makes an object a thenable, asked once for everything that asks it.
//!
//! # Why this is not part of [`super::state`]
//!
//! Two operations need the answer and they need different halves of it.
//! `state::resolve` needs the `then` ITSELF, to hand to the microtask that will
//! call it; `finally` needs only whether there is one, to decide whether it has
//! anything to wait for. Written where each is used, that is two readings of one
//! property with two chances to disagree about what counts — and the case they
//! would disagree about is precisely the one that was wrong before this module
//! existed: a `then` behind a getter.
//!
//! It also came out because `state` had reached the crate's 500-line ceiling,
//! and this is the piece of it that is a question rather than a table.

use rts_cranelift::sched::PromiseId;

use crate::entry::Context;
use crate::value::Value;

use super::state;

/// What a cell's `then` says about resolving a promise with it.
///
/// # Why three answers and not an `Option`
///
/// Because a `then` that is an ACCESSOR cannot be read where the question is
/// asked. This used to go through `objects::read_property`, which answers data
/// properties only — so `Promise.resolve({ get then() { … } })` saw no `then` at
/// all, fulfilled with the object, and the getter was never run. Silently: the
/// object came back out of the `.then` handler looking like an ordinary value.
///
/// Collapsing the getter case into "absent" is that bug; collapsing it into
/// "present" is worse, because the value behind it may not be callable and then
/// the promise must fulfil with the object after all. So the READ is what gets
/// deferred, and the third answer is what carries that decision to the drain.
pub(super) enum Then {
    /// A callable `then`, already read from a data property.
    Ready(u64),
    /// A `then` behind a getter: reading it is user code, so the drain does it.
    Deferred,
    /// No `then` anywhere on the chain, or one that is not callable.
    Absent,
}

/// Which of the three a cell is.
///
/// Through `accessor::resolve` rather than `objects::read_property`, because
/// that is the crate's one answer to "what does this key resolve to, data or
/// accessor" — it walks the accessors and the layouts together, one cell at a
/// time, which is the only order that gets shadowing right. Asking the two
/// tables separately here would be a second walk that could disagree with the
/// one every ordinary property read uses.
pub(super) fn then_of(context: &mut Context, cell: u32) -> Then {
    let key = context.well_known("then");
    match crate::entry::accessor::resolve(context, cell, key) {
        crate::entry::accessor::Found::Getter(_) => Then::Deferred,
        crate::entry::accessor::Found::Value(found) => match Value(found)
            .as_slot()
            .and_then(|callee| context.callable_at(callee))
        {
            Some(_) => Then::Ready(found),
            None => Then::Absent,
        },
        crate::entry::accessor::Found::Absent => Then::Absent,
    }
}

/// The promise a value has to be waited on THROUGH, or `None` for one that is
/// already an answer.
///
/// Asks [`state::resolve`]'s own two questions in its own order — is this a
/// promise, is this a thenable — because the caller is `finally`, which has to
/// know whether its callback answered something to wait for.
///
/// # Why `None` matters, measured
///
/// The first version had `finally` wait unconditionally — resolve a fresh
/// promise with whatever came back and react to it — on the grounds that
/// `PromiseResolve(C, result).then(…)` is what the specification writes. It is,
/// and it is still one microtask too many: measured against Bun 2026-08-14,
/// `p.finally(() => {}).then(g)` runs `g` on the same turn as `p.then(f).then(g)`
/// does, and the unconditional version put it a whole turn behind. That is
/// observable to any program with two chains in flight, which is what a program
/// using `finally` for cleanup usually is.
///
/// A promise is answered AS IT STANDS for the same reason — wrapping it in
/// another would spend a second microtask arriving at a value it already has.
pub(super) fn waited_on(context: &mut Context, value: u64) -> Option<PromiseId> {
    let cell = Value(value).as_slot()?;
    if let Some(id) = context.promises.id_of(cell) {
        return Some(id);
    }
    if matches!(then_of(context, cell), Then::Absent) {
        return None;
    }
    // A foreign thenable is not a promise yet, and `resolve` is the one thing
    // that turns one into the other — including the deferred `then` read, which
    // is why this hands the value over rather than reading it again here.
    let (_, inner) = state::fresh(context)?;
    state::resolve(context, inner, value);
    Some(inner)
}
