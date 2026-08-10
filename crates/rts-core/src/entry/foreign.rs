//! A word a client keeps beside an object, which the runtime never reads.
//!
//! # What this is and what it is not
//!
//! It is one machine word, attached to a cell, and opaque here. It is NOT a
//! reference: nothing marks it, nothing follows it, and a value encoded into it
//! would be invisible to the collector — see [`super::external`] for what to
//! use when the thing outside must be KEPT, and [`super::weak`] for when it
//! must be watched.
//!
//! # Why an `Aside` and not a field, a property, or a new heap kind
//!
//! `Aside<T>` is how this crate already says "state beside a cell", eighteen
//! times over — prototypes, callables, proxies, views, generators. A new
//! `Context` field of the same shape would be the duplicate its own rules keep
//! naming, and the sweep already knows how to clear an `Aside`.
//!
//! A property was the alternative and is wrong twice: it would be visible to
//! the program (`Object.keys` would show it) and it would have to hold a
//! VALUE, which a raw pointer is not.
//!
//! The engine being replaced answered this with a heap-entry kind
//! (`Entry::NapiExternal`) — a variant of the tagged enum its heap was. This
//! heap is cells and shapes with no variant to add, which is why the mechanism
//! differs rather than being ported.
//!
//! # Who wants it
//!
//! `rts-napi-rwk`'s `napi_wrap` and `napi_create_external`: a C addon puts its
//! own `struct` behind a JavaScript object and reads it back on every later
//! call.
//!
//! # What happens when the object dies
//!
//! The word is dropped with the cell, and **nothing is called**. An addon that
//! asked for a finalizer does not get one from here yet — running it is the
//! collector hook `FinalizationRegistry` waits on too, and neither exists. What
//! this module guarantees is narrower and worth stating exactly: the word never
//! outlives the object, so it can never be read against a cell that has become
//! something else.

use super::Context;
use crate::value::Value;

/// Attaches `word` to the object `value` names, answering what was there.
///
/// `None` when the value names no cell — a number has nowhere to keep it.
pub fn attach(context: &mut Context, value: u64, word: usize) -> Option<usize> {
    let cell = Value(value).as_slot()?;
    let previous = context.foreign.copied(cell);
    context.foreign.set(cell, word);
    previous
}

/// The word attached to what `value` names.
pub fn attached(context: &Context, value: u64) -> Option<usize> {
    context.foreign.copied(Value(value).as_slot()?)
}

/// Takes the word off, answering it.
pub fn detach(context: &mut Context, value: u64) -> Option<usize> {
    let cell = Value(value).as_slot()?;
    context.foreign.remove(cell)
}

/// [`attach`], for a caller with no `Context` in hand.
pub fn attach_current(value: u64, word: usize) -> Option<usize> {
    super::with_current(|context| attach(context, value, word))
}

/// [`attached`], for a caller with no `Context` in hand.
pub fn attached_current(value: u64) -> Option<usize> {
    super::with_current(|context| attached(context, value))
}

/// [`detach`], for a caller with no `Context` in hand.
pub fn detach_current(value: u64) -> Option<usize> {
    super::with_current(|context| detach(context, value))
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

    fn object(context: &mut Context) -> u64 {
        let ty = context.types.declare(&[rts_cranelift::repr::Repr::I64]);
        let cell = context
            .region
            .alloc(crate::heap::STRIDE, ty.index() as u32)
            .expect("room");
        Value::from_slot(cell).bits()
    }

    #[test]
    fn a_value_with_no_cell_has_nowhere_to_keep_a_word() {
        let mut context = context();
        assert_eq!(attach(&mut context, Value::from_f64(1.0).bits(), 7), None);
    }

    #[test]
    fn attaching_twice_answers_what_was_displaced() {
        // The ABI's `napi_wrap` refuses a second wrap, and it can only refuse
        // what it is told about — so replacing has to report rather than
        // silently overwrite a pointer the addon still owns.
        let mut context = context();
        let value = object(&mut context);
        assert_eq!(attach(&mut context, value, 1), None);
        assert_eq!(attach(&mut context, value, 2), Some(1));
        assert_eq!(attached(&context, value), Some(2));
        assert_eq!(detach(&mut context, value), Some(2));
        assert_eq!(attached(&context, value), None);
    }
}
