//! Re-entering a parked frame, and reading what came back out of it.
//!
//! Beside [`super`] rather than in it because the two are different questions
//! and the file was over this crate's 500-line ceiling with both in it. What is
//! here is everything about ONE re-entry: whether the frame may be entered at
//! all, what the borrow of the context may not be held across, and how the
//! three [`ResumeMode`]s differ in what they answer.
//!
//! Nothing here decides what a generator IS — that is [`super`]'s: the object,
//! its frame's cell, the class it inherits from, and the `{ value, done }`
//! shape every entry answers with.

use rts_cranelift::frame::ResumeMode;

use super::{State, result};
use crate::entry::{Context, objects, throw, with_current};
use crate::value::Value;

/// The cell of a generator whose body is parked at a suspension.
///
/// `None` for anything else — not a generator, never advanced, or finished —
/// and every one of those is a case where re-entering the body would be wrong
/// rather than merely useless. See [`super`]'s note.
pub(super) fn resumable(this: u64) -> Option<u32> {
    let cell = Value(this).as_slot()?;
    with_current(|context| parked(context, cell)).then_some(cell)
}

/// Whether a generator's frame is sitting at a suspension.
///
/// Asked of the frame's own label field, which the machine writes when it
/// parks: zero is "not started", and each suspension point has a number.
/// `done` is this crate's, because a body that ran to its end left the label at
/// whatever the last suspension was.
fn parked(context: &Context, cell: u32) -> bool {
    let Some(state) = context.generators.get(cell) else {
        return false;
    };
    !state.done
        && context
            .region
            .spanning_field(state.frame, state.shape.label_field, state.shape.slots)
            .is_some_and(|label| label != 0)
}

/// Marks a generator finished, so nothing re-enters its frame.
///
/// Shared by `return` and `throw` because the completion is the same for both —
/// only what they answer differs — and writing it twice is how one of them would
/// keep letting a dead generator be advanced.
pub(super) fn finish(context: &mut Context, cell: u32) {
    if let Some(state) = context.generators.get_mut(cell) {
        state.done = true;
    }
}

/// The same, for a caller holding the object rather than its cell.
///
/// Anything that is not a cell is not a generator, and there is nothing to
/// finish — which is why this answers nothing rather than refusing.
pub(super) fn finish_value(context: &mut Context, this: u64) {
    if let Some(cell) = Value(this).as_slot() {
        finish(context, cell);
    }
}

/// What one resumption needs from a parked generator, and nothing else.
///
/// `Copy`, which is the point: it leaves the borrow of the context behind by
/// construction, without cloning the `Vec` a `FrameShape` carries. See
/// [`resume`].
#[derive(Clone, Copy)]
struct Resuming {
    /// The rewritten body, as an address.
    code: u64,
    /// The frame's cell.
    frame: u32,
    /// How wide the frame is.
    slots: u32,
    /// Where the resumed value is written.
    resumed_field: u32,
    /// Where the way this resumption was made is written.
    mode_field: u32,
    /// Where a finished body left its answer, if it has one.
    return_field: Option<u32>,
}

impl Resuming {
    /// What a parked generator's state says about entering it once.
    fn of(state: &State) -> Self {
        Self {
            code: state.code,
            frame: state.frame,
            slots: state.shape.slots,
            resumed_field: state.shape.resumed_field,
            mode_field: state.shape.mode_field,
            return_field: state.shape.return_field,
        }
    }
}

/// What one re-entry left behind, before anything dresses it as a result.
///
/// # Why this is not the `{ value, done }` object
///
/// Two clients re-enter a frame and they want different shapes. A generator's
/// `next()` wants the iterator result, which is what [`resume`] builds; an
/// async function's driver wants the three facts — a value, whether the body
/// finished, and whether it left a throw in flight — because what it does with
/// them is settle a promise rather than answer an object. Reading `value` and
/// `done` back off an object the other client just built would be one shape
/// serving as a wire format between two halves of this crate.
pub(in crate::entry) struct Advanced {
    /// What came out: the awaited or yielded value, or the body's answer.
    pub(in crate::entry) value: u64,
    /// Whether the body ran to its end rather than parking again.
    pub(in crate::entry) finished: bool,
    /// Whether it left by throwing, which leaves the throw in flight.
    pub(in crate::entry) threw: bool,
}

/// Re-enters a generator's body and answers what came out of it.
///
/// The iterator-protocol dressing of [`advance`], and nothing else: everything
/// about the re-entry is there, because an async function's driver performs the
/// same re-entry and answers a promise instead of an object.
pub(super) fn resume(cell: u32, sent: u64, mode: ResumeMode) -> u64 {
    let stepped = advance(cell, sent, mode);
    if stepped.threw {
        return stepped.value;
    }
    with_current(|context| result(context, stepped.value, stepped.finished))
}

/// Re-enters a parked frame once.
///
/// The borrow is dropped before the body runs. Compiled code allocates, reads
/// properties and calls back into this crate, all of which take the context —
/// and a second borrow inside an `extern "C"` frame aborts the process rather
/// than unwinding.
pub(in crate::entry) fn advance(cell: u32, sent: u64, mode: ResumeMode) -> Advanced {
    // The SCALARS the resumption needs, not a clone of the whole state.
    //
    // `State` holds a `FrameShape`, which holds `param_fields: Vec<u32>` — so
    // cloning it allocated on the Rust heap on every single `.next()`, to
    // carry a list this function never reads. The clone was there to end the
    // borrow before the body runs, which is required and is what `Resuming`
    // keeps: it is `Copy`, so it leaves the borrow behind by construction.
    let entered = with_current(|context| {
        let state = context.generators.get(cell)?;
        if state.done {
            return None;
        }
        let resuming = Resuming::of(state);
        // Which generator is being re-entered, for the length of the body. A
        // stack because the calls are: a `yield*` steps its inner iterator from
        // inside this body, and if that iterator is itself a generator its own
        // re-entry pushes and pops before this one ends. See `super::delegate`.
        context.resuming.push(cell);
        // A delegation belongs to the turn that established it. Cleared on the
        // way in rather than trusted to be cleared on the way out — see
        // `super::delegate::entering` for the paths that would otherwise leave
        // a stale one behind.
        super::delegate::entering(context, cell);
        context.region.set_spanning_field(
            resuming.frame,
            resuming.resumed_field,
            resuming.slots,
            sent,
        );
        // Written on every resumption, ordinary ones included, so that the mode
        // a previous abrupt resumption left behind cannot be read as this one's.
        context.region.set_spanning_field(
            resuming.frame,
            resuming.mode_field,
            resuming.slots,
            mode.number(),
        );
        Some(resuming)
    });
    let Some(state) = entered else {
        return Advanced {
            value: with_current(|context| objects::undefined_of(context)),
            finished: true,
            threw: false,
        };
    };

    // SAFETY: the address came from the host's own record of what it placed, and
    // the shape is the one `resumable_form` states — one reference in, whether it
    // finished out.
    let body: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(state.code as usize) };
    let finished = body(i64::from(state.frame)) != 0;

    // Rule 8 of `crates/rts-core/README.md`, and the body IS the user code this
    // native called. A generator whose body threw is COMPLETED — `.next()` on it
    // afterwards answers `{ done: true }` and never runs another statement —
    // where this used to read the frame's return field and re-enter on the next
    // call, throwing the same error again for as long as it was asked. The
    // throw is left in flight rather than taken: the compiled call site that
    // asked for the next element re-raises it, which is how it reaches the
    // program's own `try`.
    //
    // A resumption of `ResumeMode::Unwind` that no `try` in the body caught
    // arrives here too, and needs nothing of its own: `g.throw(e)` handing the
    // error back out IS the throw the body did not stop.
    if throw::in_flight() {
        return with_current(|context| {
            context.resuming.pop();
            if let Some(state) = context.generators.get_mut(cell) {
                state.done = true;
            }
            Advanced {
                value: objects::undefined_of(context),
                finished: true,
                threw: true,
            }
        });
    }

    with_current(|context| {
        context.resuming.pop();
        let produced = match finished {
            // A resumption that RETURNED answers the value it was given. The
            // machine writes nothing where the function's answer goes on that
            // path, and deliberately: the value is this caller's, so asking the
            // frame to hand it back would be a round trip through a slot whose
            // representation need not even match. See `ResumeMode::Return`.
            true if mode == ResumeMode::Return => sent,
            // Whatever the body left where it returns. A generator that falls off
            // its end returns nothing, and `undefined` is what that means.
            true => state
                .return_field
                .and_then(|field| {
                    context
                        .region
                        .spanning_field(state.frame, field, state.slots)
                })
                .unwrap_or_else(|| objects::undefined_of(context)),
            // What `yield` left on its way out, taken rather than read: the next
            // resumption must not see this one's value if the body ends without
            // producing another.
            false => context
                .yielded
                .take()
                .unwrap_or_else(|| objects::undefined_of(context)),
        };
        if finished && let Some(state) = context.generators.get_mut(cell) {
            state.done = true;
        }
        Advanced {
            value: produced,
            finished,
            threw: false,
        }
    })
}
