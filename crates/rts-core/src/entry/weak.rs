//! Watching a value without keeping it.
//!
//! The mirror of [`super::external`], and the two exist for opposite halves of
//! one question. External HOLDS: the collector must keep what it names. This
//! one WATCHES: the collector may take what it names, and the watcher finds out
//! rather than reading a cell that has been handed to something else.
//!
//! # Why the answer cannot be "keep the word and check later"
//!
//! A value naming a cell is a word, and a freed cell is REUSED. So a word kept
//! across a collection is not merely stale — it may name a live object of some
//! other shape entirely, and every question asked of it answers plausibly. That
//! is the failure this module exists to make impossible: the sweep clears the
//! entry as it frees the cell, so a watcher reads absence rather than somebody
//! else's object.
//!
//! # Who wants it
//!
//! `rts-napi`'s `napi_reference` at refcount zero, which is what an addon
//! uses to cache a value it does not want to keep alive.
//!
//! `WeakRef` and `FinalizationRegistry` want the same thing and are NOT wired
//! to it here — `entry/collections/weakref.rs` still holds its target strongly
//! and says so at length. Rewiring it is a language-visible change (`deref()`
//! would start answering `undefined`, which is correct and which no fixture has
//! ever seen) and belongs in its own change, with the suite run against it. The
//! mechanism it names as missing is this one.
//!
//! # What is watched
//!
//! Only a value that names a cell. A number or a boolean is not collectable, so
//! watching one would be a subscription to an event that cannot happen —
//! [`watch`] refuses rather than pretending, and the caller decides what a
//! non-collectable value means to it.

use super::Context;
use crate::value::Value;

/// Starts watching `value`, answering the identifier that reads it back.
///
/// `None` when the value names no cell.
pub fn watch(context: &mut Context, value: u64) -> Option<u32> {
    let cell = Value(value).as_slot()?;
    let id = context.next_weak;
    context.next_weak = context.next_weak.wrapping_add(1);
    context.weak.entry(cell).or_default().push((id, Some(value)));
    Some(id)
}

/// What is watched under `id`.
///
/// `None` for an identifier this never issued or already released, and
/// `Some(None)` for one whose value has been collected — two different facts an
/// addon reports differently, which is why they are not one `Option`.
pub fn peek(context: &Context, id: u32) -> Option<Option<u64>> {
    // BY ID, so this walks where `clear_freed` does not — the same split
    // `finalize` makes, and for the same reason: an addon reading a `WeakRef`
    // asks this, where the sweep asks the other one once per freed cell.
    context
        .weak
        .values()
        .flatten()
        .find(|(watched, _)| *watched == id)
        .map(|(_, value)| *value)
}

/// Stops watching.
pub fn forget(context: &mut Context, id: u32) {
    context.weak.retain(|_, watching| {
        watching.retain(|(watched, _)| *watched != id);
        !watching.is_empty()
    });
}

/// Clears every watch on a cell the sweep is about to free.
///
/// Called from [`super::collect_cycle`], before the cell is given back, for the
/// reason that module's `release` states about every other side table: reading
/// or writing after the free is reading a cell already spliced into the free
/// list.
pub(super) fn clear_freed(context: &mut Context, cell: u32) {
    // The empty case FIRST: nothing in this repository outside `rts-napi`
    // produces a watch — `WeakRef` holds its target as an ordinary property —
    // so for every JavaScript program this is one load and one branch, asked
    // once per freed cell.
    if context.weak.is_empty() {
        return;
    }
    // CLEARED, not removed: `peek` must go on distinguishing an id that was
    // never issued from one whose value died, and dropping the entry would
    // collapse the two into `None`.
    if let Some(watching) = context.weak.get_mut(&cell) {
        for (_, value) in watching {
            *value = None;
        }
    }
}

/// [`watch`], for a caller with no `Context` in hand.
pub fn watch_current(value: u64) -> Option<u32> {
    super::with_current(|context| watch(context, value))
}

/// [`peek`], for a caller with no `Context` in hand.
pub fn peek_current(id: u32) -> Option<Option<u64>> {
    super::with_current(|context| peek(context, id))
}

/// [`forget`], for a caller with no `Context` in hand.
pub fn forget_current(id: u32) {
    super::with_current(|context| forget(context, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{Kinds, Singletons};

    fn context() -> Context {
        Context::new(
            Singletons {
                undefined: 0,
                null: 1,
                hole: 2,
            },
            Kinds::in_declaration_order(),
        )
    }

    #[test]
    fn a_value_that_cannot_be_collected_is_refused_rather_than_watched() {
        // A number never dies, so a watch on one is a subscription to an event
        // that cannot happen — and an addon told "still alive" forever by a
        // mechanism that never looked would be right by accident.
        let mut context = context();
        assert_eq!(watch(&mut context, crate::value::Value::from_f64(1.0).bits()), None);
    }

    #[test]
    fn an_unknown_identifier_and_a_collected_value_are_different_answers() {
        let mut context = context();
        let ty = context.types.declare(&[rts_cranelift::repr::Repr::I64]);
        let cell = context
            .region
            .alloc(crate::heap::STRIDE, ty.index() as u32)
            .expect("room");
        let id = watch(&mut context, Value::from_slot(cell).bits()).expect("a cell is watchable");

        assert!(matches!(peek(&context, id), Some(Some(_))), "alive");
        clear_freed(&mut context, cell);
        assert_eq!(peek(&context, id), Some(None), "collected, and the watch remains");
        forget(&mut context, id);
        assert_eq!(peek(&context, id), None, "and now the watch is gone too");
    }
}
