//! A generator: a function whose frame is parked between answers.
//!
//! The hard half is not here. `rts_cranelift::frame::resumable_form` rewrites a
//! body that may suspend into `extern "C" fn(frame) -> finished?`, and
//! `crates/rts-host/tests/generator_frame.rs` establishes that such a
//! function reads and writes a frame this crate's own heap handed out. What is
//! here is the object a program holds: the frame's cell, the code to re-enter,
//! and the `{ value, done }` each entry answers.
//!
//! # Where the yielded value comes from, and why there is no stack
//!
//! `yield x` compiles to a call that leaves `x` in [`Context::yielded`] followed
//! by a suspension. Nothing records WHICH generator produced it, and nothing
//! needs to: whoever resumed reads it the instant the call returns, and a
//! generator resumed from inside another's body has already had its own value
//! taken by then. The nesting is a stack because the calls are.
//!
//! A stack of running generators was the alternative — the shape
//! `functions::invoke` uses for callees — and it would hold the same value for
//! longer, with a discipline to keep it in step with a control flow that
//! `throw` can leave. One slot that is written and immediately taken cannot
//! drift out of step with anything.
//!
//! # Why `throw` and `return` do not re-enter the body, and what would let them
//!
//! `g.throw(e)` has to make the `yield` that parked the frame RAISE, and
//! `g.return(v)` has to make it return through every enclosing `finally`. Both
//! are things the BODY does, and this file cannot ask it to: the machine's
//! contract is `extern "C" fn(frame) -> finished?` with one delivered value, so
//! what comes back from a suspension is a value and there is no channel for
//! "and this one is a throw".
//!
//! The channel is the emitter's, and it is one operation. `yield` compiles to a
//! call to [`generator_yield`] followed by `Inst::Suspend`, and the check
//! compiled code already emits after every call is what would carry a throw —
//! but it sits BEFORE the suspension, so a resumption lands past it. What is
//! missing is an operation emitted immediately AFTER the suspension, answering
//! how this resumption was made: ordinary, a throw — which the operation itself
//! raises, so the call check that follows unwinds into the body's own `try` —
//! or a return, which the emitter turns into the function's return path so that
//! `finally` runs. This file would then write the mode beside the frame and
//! re-enter, and both methods would be [`resume`] with one field set.
//!
//! The rejected alternative was a second entry into the rewritten function:
//! `frame::resumable_form` producing a "resume abruptly" address beside the
//! plain one. It puts a language fact — that a suspension can be resumed with a
//! completion other than a value — into the machine, and doubles what the frame
//! transform has to keep sound over arbitrary control flow, for something one
//! instruction after the suspension expresses without the machine knowing why.

use super::{Context, with_current};
use crate::value::Value;

/// What a parked frame looks like, for one compiled generator body.
///
/// Every number here is `rts_cranelift::frame::FrameLayout`'s, carried across
/// rather than re-derived: this crate cannot see the function that was rewritten,
/// and a second derivation of a field index is the failure the machine layer's
/// single-source rule exists to prevent. The host fills this in, keyed by the
/// code address, because the address is the one thing the runtime already holds
/// about a compiled function.
#[derive(Clone, Debug)]
pub struct FrameShape {
    /// The address of the rewritten body, as `FuncAddr` produced it.
    pub code: u64,
    /// The type identifier the frame's header carries.
    pub ty: u32,
    /// How many bytes the frame occupies, header included.
    pub size: u32,
    /// How many fields it has — the bound the runtime's accessors take.
    pub slots: u32,
    /// Where the resume label is kept.
    pub label_field: u32,
    /// Where a resumption leaves the value it delivers.
    pub resumed_field: u32,
    /// Where the body's own parameters live, in the calling convention's order.
    pub param_fields: Vec<u32>,
    /// Where the body leaves what it returns, when it returns anything.
    pub return_field: Option<u32>,
}

/// A generator that has been made, and how to re-enter it.
///
/// Beside the cell rather than in it, the same placement `Map`'s table has: the
/// object's own slots stay ordinary properties, so a program that hangs a field
/// on a generator is not fighting the runtime for room.
#[derive(Clone, Debug)]
pub(super) struct State {
    /// The rewritten body.
    code: u64,
    /// The reference of the frame's cell.
    frame: u32,
    /// The shape that frame has.
    shape: FrameShape,
    /// Whether the body has run to its end.
    done: bool,
}

impl State {
    /// The frame's own cell reference.
    ///
    /// For the tracer: the frame is a SPANNING allocation, with no seven-slot
    /// header `Region::field` could read, so it has to be walked and kept
    /// alive through this rather than through the ordinary per-cell path.
    pub(in crate::entry) fn frame_cell(&self) -> u32 {
        self.frame
    }

    /// Every value the parked frame holds, field by field.
    ///
    /// Walked through `Region::spanning_field`, because a frame's fields
    /// continue past a cell boundary and the ordinary accessor refuses past
    /// the seventh. Every field is offered to the caller rather than filtered
    /// here: a frame's layout mixes locals that are references with ones that
    /// are not, and only `Value::kind` can tell them apart.
    pub(in crate::entry) fn trace(&self, region: &crate::heap::Region, out: &mut Vec<u64>) {
        for slot in 0..self.shape.slots {
            if let Some(word) = region.spanning_field(self.frame, slot, self.shape.slots) {
                out.push(word);
            }
        }
    }
}

/// Records what the host knows about every compiled generator body.
///
/// Called once per program, before it runs, for the same reason
/// `declare_literals` is: the addresses are fixed when the program is placed, and
/// the context that runs it is built afterwards.
pub fn declare_frames(context: &mut Context, frames: Vec<FrameShape>) {
    context.frames = frames;
}

/// Makes the generator object a call to a generator function answers.
///
/// The body does NOT run. That is the whole difference between a generator
/// function and an ordinary one, and it is expressed by what the compiler emits
/// — a wrapper that calls this instead of the body — rather than by a check on
/// the calling path. The alternative was a table consulted by
/// `functions::invoke` before every jump, which would make every ordinary call
/// pay for the existence of generators.
///
/// `code` is the address of the REWRITTEN body, and the shape is found by it.
/// An address nothing registered answers `undefined`: that is a compiler that
/// emitted a wrapper without registering the frame, and inventing a shape for it
/// would produce a generator that runs and reads the wrong words.
#[rtse::entry]
pub fn generator_new(
    code: i64,
    environment: u64,
    this: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
) -> u64 {
    with_current(|context| {
        let address = code as u64;
        let Some(shape) = context
            .frames
            .iter()
            .find(|frame| frame.code == address)
            .cloned()
        else {
            return super::objects::undefined_of(context);
        };

        // The frame spans as many cells as it needs. A generator of six
        // parameters already exceeds one, so this is the ordinary case rather
        // than the exceptional one.
        //
        // Through `alloc`, which COLLECTS and retries — not `region` directly,
        // which does not. Asking the region straight answered `None` the moment
        // the heap filled, and this function turned that into `undefined`: so
        // `g(8)` handed back a value that is not a generator, `it.next` read
        // absent off it, and the program died with `TypeError: undefined is not
        // a function` while a collection would have freed 60 000 dead frames.
        // Measured: a loop of 60 000 generators, every time.
        //
        // `_or_die` rather than answering `undefined` on genuine exhaustion,
        // because that is what the rest of the crate does and because a silent
        // wrong value is worse than a stop that says why — the honesty floor
        // applied to an allocation.
        let frame = super::alloc::alloc_spanning_or_die(context, shape.size, shape.ty);

        // The body is entered afresh every time, with nothing in the registers
        // it had before — so its arguments are written down now, in the order
        // the convention passes them.
        let arguments = [environment, this, a0, a1, a2, a3];
        for (field, value) in shape.param_fields.iter().zip(arguments) {
            context
                .region
                .set_spanning_field(frame, *field, shape.slots, value);
        }
        context
            .region
            .set_spanning_field(frame, shape.label_field, shape.slots, 0);

        let Some(cell) = made(context) else {
            return super::objects::undefined_of(context);
        };
        context.generators.set(
            cell,
            State {
                code: address,
                frame,
                shape,
                done: false,
            },
        );
        Value::from_slot(cell).bits()
    })
}

/// Leaves what `yield x` produced where whoever resumed will read it.
///
/// Answers its argument so that the compiler can emit it as an expression, and
/// the answer is deliberately not what `yield` evaluates to — that is the value
/// the NEXT resumption delivers, and it comes out of the suspension itself.
#[rtse::entry]
pub fn generator_yield(value: u64) -> u64 {
    with_current(|context| context.yielded = Some(value));
    value
}

/// A generator object.
///
/// `next`, `return` and `throw` are what the iterator protocol asks of one.
/// `Symbol.iterator` is installed by [`register`] rather than declared here,
/// because a class member is named by a string and that key is a symbol.
#[rtse::class("Generator")]
impl Generator {
    /// `g.next(v)` — runs until the next `yield`, or until the body ends.
    fn next(this: u64, sent: u64) -> u64 {
        let Some(cell) = Value(this).as_slot() else {
            return with_current(|context| super::objects::undefined_of(context));
        };
        resume(cell, sent)
    }

    /// `g.return(v)` — abandons the generator, answering `{ v, done: true }`.
    ///
    /// The frame is dropped where it is rather than resumed to run its `finally`
    /// blocks. That is a real difference from the specification and it is
    /// written here rather than hidden — the module's own note has the one thing
    /// missing that would close it.
    #[js("return")]
    fn returned(this: u64, value: u64) -> u64 {
        with_current(|context| {
            finish(context, this);
            result(context, value, true)
        })
    }

    /// `g.throw(e)` — reports an error INTO a generator.
    ///
    /// The body is not re-entered, so a `catch` or a `finally` written around
    /// the `yield` does not run; see the module note. What is left is the case
    /// where the body would not have caught it, and there the language says the
    /// generator completes and the error comes back out of `.throw()` — so it is
    /// **raised** rather than answered.
    ///
    /// Answering `{ value: e, done: true }` was the alternative, and it is the
    /// wrong half to keep: it makes `g.throw(e)` a way of *hiding* an error,
    /// which no program can be written against, where raising is right whenever
    /// the body has no handler and is what the tests that drive a generator by
    /// hand expect. A generator that would have caught it fails visibly instead
    /// of continuing with a wrong answer.
    #[js("throw")]
    fn thrown(this: u64, error: u64) -> u64 {
        with_current(|context| {
            finish(context, this);
            super::throw::throw_value(error);
            super::objects::undefined_of(context)
        })
    }
}

/// Marks a generator finished, so nothing re-enters its frame.
///
/// Shared by `return` and `throw` because the completion is the same for both —
/// only what they answer differs — and writing it twice is how one of them would
/// keep letting a dead generator be advanced.
fn finish(context: &mut Context, this: u64) {
    if let Some(cell) = Value(this).as_slot()
        && let Some(state) = context.generators.get_mut(cell)
    {
        state.done = true;
    }
}

/// Re-enters a generator's body and answers what came out of it.
///
/// The borrow is dropped before the body runs. Compiled code allocates, reads
/// properties and calls back into this crate, all of which take the context —
/// and a second borrow inside an `extern "C"` frame aborts the process rather
/// than unwinding.
///
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
    /// Where a finished body left its answer, if it has one.
    return_field: Option<u32>,
}

fn resume(cell: u32, sent: u64) -> u64 {
    // The five SCALARS the resumption needs, not a clone of the whole state.
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
        let resuming = Resuming {
            code: state.code,
            frame: state.frame,
            slots: state.shape.slots,
            resumed_field: state.shape.resumed_field,
            return_field: state.shape.return_field,
        };
        context.region.set_spanning_field(
            resuming.frame,
            resuming.resumed_field,
            resuming.slots,
            sent,
        );
        Some(resuming)
    });
    let Some(state) = entered else {
        return with_current(|context| {
            let absent = super::objects::undefined_of(context);
            result(context, absent, true)
        });
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
    if super::throw::in_flight() {
        return with_current(|context| {
            if let Some(state) = context.generators.get_mut(cell) {
                state.done = true;
            }
            super::objects::undefined_of(context)
        });
    }

    with_current(|context| {
        let produced = match finished {
            // Whatever the body left where it returns. A generator that falls off
            // its end returns nothing, and `undefined` is what that means.
            true => state
                .return_field
                .and_then(|field| {
                    context
                        .region
                        .spanning_field(state.frame, field, state.slots)
                })
                .unwrap_or_else(|| super::objects::undefined_of(context)),
            // What `yield` left on its way out, taken rather than read: the next
            // resumption must not see this one's value if the body ends without
            // producing another.
            false => context
                .yielded
                .take()
                .unwrap_or_else(|| super::objects::undefined_of(context)),
        };
        if finished && let Some(state) = context.generators.get_mut(cell) {
            state.done = true;
        }
        result(context, produced, finished)
    })
}

/// The `{ value, done }` an iterator answers with.
///
/// Shared with `super::list_iterator`, which answers the same shape: two
/// spellings of one object is how the two would come to disagree about whether
/// an exhausted iterator carries `undefined` or nothing at all.
pub(in crate::entry) fn result(context: &mut Context, value: u64, done: bool) -> u64 {
    let Some(cell) = made(context) else {
        return super::objects::undefined_of(context);
    };
    let key = context.well_known("value");
    super::objects::put(context, cell, key, value);
    let key = context.well_known("done");
    super::objects::put(context, cell, key, Value::from_bool(done).bits());
    Value::from_slot(cell).bits()
}

/// A plain object inheriting from the generator prototype.
///
/// Registering on first use rather than at start-up, the same way every class
/// here is reached: a program with no generator in it pays for none of this.
fn made(context: &mut Context) -> Option<u32> {
    let cell = super::native::plain(context)?;
    let prototype = match super::class_support::prototype(context, "Generator") {
        Some(prototype) => prototype,
        None => {
            register(context);
            super::class_support::prototype(context, "Generator")?
        }
    };
    context.set_prototype(cell, prototype);
    Some(cell)
}

/// Installs the class, and the one member its attribute cannot name.
///
/// `Symbol.iterator` answering the generator itself is what makes `for (const x
/// of g())` work: the protocol asks the value for an iterator, and a generator
/// IS one.
pub(in crate::entry) fn register(context: &mut Context) -> u64 {
    let made = register_generator(context);
    if let Some(prototype) = super::class_support::prototype(context, "Generator")
        && let Some(cell) = Value(prototype).as_slot()
    {
        let key = context.well_known(&format!("{}iterator", super::symbol::PREFIX));
        let itself = super::native::callable(context, itself as super::native::Native);
        super::objects::put(context, cell, key, itself);
    }
    made
}

/// `g[Symbol.iterator]()` — the generator itself.
extern "C" fn itself(_environment: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}
