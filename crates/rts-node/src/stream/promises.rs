//! `node:stream/promises` — `pipeline` and `finished`, as promises.
//!
//! # Why this is a layer and not a second implementation
//!
//! Because in Node it is one: `stream/promises` exports the same two operations
//! as `node:stream`, differing only in how the answer arrives. So this mints a
//! promise, calls [`super::util`]'s callback form with a callback that settles
//! it, and answers the promise. Nothing here knows what a pipeline IS.
//!
//! Writing it the other way round — a promise implementation beside the
//! callback one — is what makes two answers to "did this stream finish", and
//! they disagree the first time either learns about an error the other does
//! not.
//!
//! # What the reuse check found
//!
//! `entry::promise_new` and `entry::promise_settle` already exist, and
//! `node:timers/promises` uses exactly this shape — its own doc records that
//! the refusal which preceded it was stale rather than wrong. `super::util`'s
//! `pipeline`/`finished` already attach `'end'`/`'error'` listeners, which is
//! the whole of what this needs.
//!
//! # The one thing to know about the answer
//!
//! `pipeline` fulfils with `undefined`, not with the last stream. That is
//! Node's contract and it differs from the callback form, which answers the
//! stream synchronously so a caller can keep piping — the two are not the same
//! function with a different tail.

use rts_core::entry::{self, Provided};

/// The members `node:stream/promises` is.
pub(super) const MEMBERS: &[(&str, Provided)] = &[("pipeline", pipeline), ("finished", finished)];

/// `pipeline(...streams)` — a promise for the whole chain finishing.
extern "C" fn pipeline(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    // Minted first and OUTSIDE any borrow: `promise_new` takes the runtime
    // borrow itself, and taking it twice aborts the process rather than
    // failing — the note `node:timers/promises` carries for the same call.
    let promise = entry::promise_new();
    let settler = entry::closure_new(settle as *const () as usize as i64, promise);
    // The callback goes in the first free slot after the streams, which is
    // where the callback form looks for it: it takes the LAST callable of the
    // four and treats the rest as streams.
    let slots = place(settler, [a, b, c, d]);
    super::util::pipeline(
        entry::undefined_value(),
        entry::undefined_value(),
        slots[0],
        slots[1],
        slots[2],
        slots[3],
    );
    promise
}

/// `finished(stream)` — a promise for one stream ending.
extern "C" fn finished(_e: u64, _this: u64, stream: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let promise = entry::promise_new();
    let settler = entry::closure_new(settle as *const () as usize as i64, promise);
    let absent = entry::undefined_value();
    super::util::finished(absent, absent, stream, settler, absent, absent);
    promise
}

/// Puts `callback` after the given arguments, in the four slots a native has.
///
/// # Why the position matters
///
/// [`super::util::pipeline`] finds the callback as the last callable of its
/// four arguments and treats everything else as a stream. A callback placed
/// before a stream would therefore make that stream the callback and this
/// promise never settle — so it goes strictly after the last argument that is
/// there. Four is the whole budget: a pipeline of more than three streams
/// loses the callback slot, and that is a limit of the calling convention
/// rather than of this file, named here because this is where it bites.
fn place(callback: u64, given: [u64; 4]) -> [u64; 4] {
    let absent = entry::undefined_value();
    let mut slots = [absent; 4];
    let mut at = 0;
    for value in given {
        if value == absent {
            break;
        }
        slots[at] = value;
        at += 1;
        if at == slots.len() {
            // No room left. The pipeline still runs — it is the promise that
            // cannot be settled, which is why this is stated above rather than
            // silently dropped.
            return slots;
        }
    }
    slots[at] = callback;
    slots
}

/// Settles the promise this callback was minted for.
///
/// The callback form hands an error or nothing, so the first argument decides:
/// a value means rejection, its absence means fulfilment. `1` and `0` are the
/// settlement kinds `entry::promise_settle` takes.
extern "C" fn settle(promise: u64, _this: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let is_error = error != absent && error != entry::null_value();
    match is_error {
        true => entry::promise_settle(promise, error, 1),
        // `undefined` and not the stream: Node's promise form fulfils with
        // nothing, where the callback form answers the last stream so a caller
        // can keep piping. The two differ here and this is the difference.
        false => entry::promise_settle(promise, absent, 0),
    }
    absent
}
