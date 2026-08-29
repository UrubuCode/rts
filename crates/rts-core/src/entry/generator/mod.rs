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
//! # How `throw` and `return` re-enter the body
//!
//! `g.throw(e)` has to make the `yield` that parked the frame RAISE, and
//! `g.return(v)` has to make it return through every enclosing `finally`. Both
//! are things the BODY does, so both are one field: whoever resumes writes a
//! [`rts_cranelift::frame::ResumeMode`] beside the value it delivers, and the
//! dispatch the frame transform emits at every suspension point reads it and
//! either carries on, throws AT the suspension, or returns from there. All
//! three methods are [`resume`] with one number changed.
//!
//! Throwing at the suspension rather than from here is the whole point. A
//! `throw` this file raised would land in whatever regions THIS call is inside,
//! which are the resumer's; raised where the frame parked, it lands in the
//! regions the `yield` was written in, so a `try` around it catches and the
//! generator carries on. The same for a return: it is a `Return` inside those
//! regions, so lowering runs what leaving them owes.
//!
//! # What must be decided before re-entering, and why
//!
//! A generator that has not been entered yet has no suspension outstanding —
//! the dispatch would land on the body's beginning, where nothing reads the
//! mode — so re-entering one with an abrupt mode would RUN it from the top.
//! [`resume::resumable`] is the question, and it is asked of the frame's own
//! label field rather than of a second flag here: the label is what the machine
//! writes when it parks, and a flag beside it is a second answer to one
//! question.
//!
//! The rejected alternative was an operation the emitter puts after every
//! suspension, asking a runtime how this resumption was made. It is the same
//! three answers, arrived at once per suspension point per client instead of
//! once in the rewrite that already owns those points — and the one that
//! matters, "return through the enclosing `finally`", is not something an
//! emitted call can express: it is a `Return` in the region, which is a
//! terminator, not a value.
//!
//! # What is in [`resume`] and not here
//!
//! One re-entry: whether the frame may be entered, what the borrow of the
//! context may not be held across, and how the three modes differ in what they
//! answer. Split off because this file was over the crate's 500-line ceiling
//! with both in it, and because the seam is real — everything here is about
//! what a generator IS.
//!
//! # What re-entering does NOT yet reach, and whose it is
//!
//! Two things, and neither is this file's — written here because this is where
//! a reader will look for them.
//!
//! A `finally` that can itself suspend is emitted as a catch-all HANDLER rather
//! than as a cleanup (`emit/protect.rs` says why: the frame rewrite turns each
//! suspension into a return, and a return has no place in a cleanup copy). A
//! handler catches throws, so `ResumeMode::Return` walks straight past it —
//! `try { yield 1 } finally { yield 99 }` runs its `finally` on `g.throw(e)`
//! and not on `g.return(v)`. Closing it needs a pending completion the language
//! layer carries across the `finally`, which is `emit/protect.rs`'s.
//!
//! `yield*` does not forward `return`/`throw` to the inner iterator. The
//! resumption now reaches the delegating `yield`, so the outer generator does
//! the right thing with it — but the specification says the INNER iterator's
//! own `return`/`throw` is called first, and that is the shape of the loop
//! `emit/delegate.rs` emits.

mod delegate;
mod resume;

use rts_cranelift::frame::ResumeMode;

pub use self::delegate::{DELEGATE_STEP_ENTRY, delegate_step};
use self::resume::{finish_value, refuse_running, resumable, resume, running};
/// One re-entry of a parked frame, for the driver that is not the iterator
/// protocol: an async function's body is the same frame and is entered the same
/// way, and only what happens to the answer differs. See `promise::async_fn`.
pub(in crate::entry) use self::resume::advance;
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
    /// Where a resumption leaves the way it was made.
    pub mode_field: u32,
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
    /// The iterator a `yield*` is standing in front of, while it is standing.
    ///
    /// Here rather than in [`Context`] because it is a fact about ONE generator
    /// and outlives the re-entry that established it: `outer.return(v)` asks
    /// about a frame that has been parked since some earlier call. A slot in
    /// the context would be the shape `yielded` has, and `yielded` is written
    /// and taken within one re-entry — which is exactly the property this does
    /// not have.
    delegating: Option<u64>,
    /// The iterator result a forwarded `throw` already obtained.
    ///
    /// Handed to the next [`delegate_step`] instead of stepping, because the
    /// step for that turn has happened: the inner iterator was advanced from
    /// outside the parked frame. See [`delegate`].
    pending: Option<u64>,
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
        // What a `yield*` is delegating to, and what a forwarded `throw` left
        // for the next step. Neither is in the frame — the frame holds the
        // iterator the BODY spilled, and these are the runtime's own record of
        // the same delegation — so nothing else names them while the generator
        // is parked.
        out.extend(self.delegating);
        out.extend(self.pending);
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
                delegating: None,
                pending: None,
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
        if running(this) {
            return refuse_running();
        }
        let Some(cell) = Value(this).as_slot() else {
            return with_current(|context| super::objects::undefined_of(context));
        };
        resume(cell, sent, ResumeMode::Deliver)
    }

    /// `g.return(v)` — returns from the `yield` the body is parked at.
    ///
    /// So every `finally` between that `yield` and the end of the body runs, and
    /// the answer is `{ v, done: true }` when they all complete normally. A
    /// `finally` that yields parks the frame again instead, and then this
    /// answers what it yielded with `done: false` — which is not a special case
    /// here at all: the frame parked, so [`resume`] reports a park.
    ///
    /// A generator that never started, or that has finished, has no `yield` to
    /// return from. It is completed without being entered, which is also what
    /// the language says — the body of a generator that was never advanced does
    /// not run because `.return()` was called on it.
    #[js("return")]
    fn returned(this: u64, value: u64) -> u64 {
        if running(this) {
            return refuse_running();
        }
        // A generator parked inside a `yield*` owes the INNER iterator its
        // `return` first, and what that answers decides whether the outer's own
        // return completion happens at all. `None` means it did not decide —
        // there was no delegation, or the inner has no `return` — so the
        // ordinary completion below proceeds.
        if let Some((cell, inner)) = delegate::delegated(this)
            && let Some(answered) = delegate::forward_return(cell, inner, value)
        {
            return answered;
        }
        match resumable(this) {
            Some(cell) => resume(cell, value, ResumeMode::Return),
            None => with_current(|context| {
                finish_value(context, this);
                result(context, value, true)
            }),
        }
    }

    /// `g.throw(e)` — raises AT the `yield` the body is parked at.
    ///
    /// A `try` written around that `yield` catches it and the generator carries
    /// on, which is the whole reason this re-enters rather than answering from
    /// out here; see the module note. A body with no handler for it lets the
    /// throw escape, and [`resume`] leaves it in flight for the call site above
    /// to re-raise — so `g.throw(e)` still ends the generator and hands the
    /// error back out when nothing caught it.
    ///
    /// A generator that never started, or that has finished, has no `yield` to
    /// raise at, and the language says the error simply comes back out.
    /// Answering `{ value: e, done: true }` was the alternative and is the wrong
    /// half to keep: it makes `g.throw(e)` a way of *hiding* an error, which no
    /// program can be written against.
    #[js("throw")]
    fn thrown(this: u64, error: u64) -> u64 {
        if running(this) {
            return refuse_running();
        }
        // The delegated iterator's own `throw` runs first, and the outer frame
        // is re-entered only if it says the delegation is over. See
        // [`delegate::forward_throw`], which always decides — an inner iterator
        // with no `throw` is closed and refused rather than skipped.
        if let Some((cell, inner)) = delegate::delegated(this)
            && let Some(answered) = delegate::forward_throw(cell, inner, error)
        {
            return answered;
        }
        match resumable(this) {
            Some(cell) => resume(cell, error, ResumeMode::Unwind),
            None => {
                with_current(|context| finish_value(context, this));
                super::throw::throw_value(error);
                with_current(|context| super::objects::undefined_of(context))
            }
        }
    }
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
    // Asked of the memo first, because the fallback is a linear walk of
    // `classes` comparing a `&str` per entry and this runs once per STEP of
    // every iterator in the program — an array cursor, a `Map`, a generator,
    // every position of every destructuring pattern. The registration it finds
    // cannot move: nothing removes an entry, so the name is asked at most once
    // per process instead of once per element.
    let prototype = match context.generator_result_prototype {
        Some(prototype) => prototype,
        None => {
            let found = match super::class_support::prototype(context, "Generator") {
                Some(prototype) => prototype,
                None => {
                    register(context);
                    super::class_support::prototype(context, "Generator")?
                }
            };
            context.generator_result_prototype = Some(found);
            found
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

        // `Symbol.toStringTag`, so `Object.prototype.toString.call(g())` answers
        // `[object Generator]`. It went to `[object Object]`, which is the
        // fallback `object_proto`'s per-kind table gives anything it does not
        // recognise — and a generator is exactly the case that table cannot
        // recognise, because a generator object has no internal slot this engine
        // records and is otherwise an ordinary object with a prototype.
        //
        // On the PROTOTYPE rather than each instance: every generator shares it,
        // a tag is a fact about the kind rather than the object, and a program
        // that makes a million of them should not pay a property for each.
        let tag = context.well_known(&format!("{}toStringTag", super::symbol::PREFIX));
        let value = context.intern_value(crate::text::Str::from_str("Generator")).bits();
        super::objects::put(context, cell, tag, value);
        super::native::hidden(context, cell, tag);
    }
    // `%GeneratorPrototype%` sits on `%IteratorPrototype%`, which is what makes
    // `g().map(f)` a thing at all — it was a `TypeError` for as long as the
    // helpers were copied onto each kind of iterator instead of inherited.
    super::iterator::adopt(context);
    made
}

/// `g[Symbol.iterator]()` — the generator itself.
extern "C" fn itself(_environment: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}
