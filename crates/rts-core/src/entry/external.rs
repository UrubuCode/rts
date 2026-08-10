//! A value something outside the heap is holding.
//!
//! # The gap this closes
//!
//! [`super::roots`] finds two things: what the [`Context`] holds directly, and
//! what a conservative scan of the machine stack finds. Between them they cover
//! every value a running program can reach — as long as the thing holding it is
//! a frame.
//!
//! A native that keeps a value AFTER its call returns is not a frame. An N-API
//! addon does exactly this: `napi_create_reference` hands a `.node` a handle it
//! may use on the next call, three turns of the event loop later, from a
//! callback the runtime knows nothing about. That handle lives in the addon's
//! own memory, which no scan of ours reads, so the value it names is
//! unreachable the moment the call that produced it returns — and the collector
//! is right to take it.
//!
//! # Why an identifier rather than a pointer
//!
//! The obvious alternative is handing out a `*mut u64` into a slot the runtime
//! owns, which is what V8 does for `Local<>` and what the addon's ABI expects to
//! be pointer-shaped. That is the CALLER's problem to solve — `rts-napi` builds
//! its pointer-shaped handles over this — and doing it here would put a
//! stability requirement on a `Vec` inside `Context`, where a single `push` at
//! the wrong moment invalidates every handle already handed out. A number
//! survives reallocation; a pointer into a growable table does not.
//!
//! # What it is not
//!
//! A reference count, and not a finalizer either. Holding twice and releasing
//! once leaves the value held, because two holds are two identifiers. What runs
//! when a value is finally collected is a separate question this does not
//! answer — see `crates/rts-napi/README.md` for who is waiting on it.

use super::{Context, with_current};

/// Keeps `value` alive until [`release`] is called with the answer.
///
/// The identifier is never reused, so one presented after its release is
/// refused rather than naming whatever took its place.
pub fn hold(context: &mut Context, value: u64) -> u32 {
    let id = context.next_external;
    context.next_external = context.next_external.wrapping_add(1);
    context.external.push((id, value));
    id
}

/// Stops keeping it, and answers what was held.
///
/// `None` for an identifier this never issued or already released — a caller
/// releasing twice is a bug in the caller, and answering `Some` for the second
/// one would hide it.
pub fn release(context: &mut Context, id: u32) -> Option<u64> {
    let at = context.external.iter().position(|(held, _)| *held == id)?;
    Some(context.external.remove(at).1)
}

/// What is held under an identifier, without releasing it.
pub fn held(context: &Context, id: u32) -> Option<u64> {
    context
        .external
        .iter()
        .find(|(held, _)| *held == id)
        .map(|(_, value)| *value)
}

/// [`hold`], for a caller that has no `Context` in hand.
///
/// The shape every entry point takes: reach the thread's context, do the one
/// thing, give the borrow back. A native must not hold this borrow across a
/// call into user code — rule 8 of this crate's README.
pub fn hold_current(value: u64) -> u32 {
    with_current(|context| hold(context, value))
}

/// [`release`], for a caller that has no `Context` in hand.
pub fn release_current(id: u32) -> Option<u64> {
    with_current(|context| release(context, id))
}

/// [`held`], for a caller that has no `Context` in hand.
pub fn held_current(id: u32) -> Option<u64> {
    with_current(|context| held(context, id))
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
            Kinds {
                symbol: 3,
                bigint: 4,
            },
        )
    }

    #[test]
    fn a_released_identifier_is_refused_rather_than_reused() {
        // The failure this prevents is the worst kind an addon can hit: a stale
        // handle that answers a DIFFERENT live value instead of failing.
        let mut context = context();
        let first = hold(&mut context, 111);
        assert_eq!(release(&mut context, first), Some(111));
        let second = hold(&mut context, 222);
        assert_ne!(first, second, "an identifier is never handed out twice");
        assert_eq!(
            held(&context, first),
            None,
            "a released identifier names nothing"
        );
    }

    #[test]
    fn releasing_twice_says_so() {
        let mut context = context();
        let id = hold(&mut context, 7);
        assert_eq!(release(&mut context, id), Some(7));
        assert_eq!(
            release(&mut context, id),
            None,
            "the second release is the caller's bug and has to be visible"
        );
    }

    #[test]
    fn one_hold_per_call_and_not_a_count() {
        // Stated as a test because the alternative — refcounting — is what a
        // reader coming from V8 will assume, and assuming it here loses values.
        let mut context = context();
        let first = hold(&mut context, 9);
        let second = hold(&mut context, 9);
        assert_eq!(release(&mut context, first), Some(9));
        assert_eq!(
            held(&context, second),
            Some(9),
            "the second hold is its own, not a count on the first"
        );
    }
}
