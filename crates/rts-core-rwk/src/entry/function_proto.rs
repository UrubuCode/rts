//! `Function.prototype` — what every callable inherits from.
//!
//! # Why the link is substituted rather than stored
//!
//! For the reason a string's is. `closure_new` runs at every function
//! definition, and writing a prototype link there would spend a word per
//! function to record one fact all of them share — and it would have to be
//! written while the prototype object itself is being built, which is the
//! recursion `string::prototype_of` records paying for once already.
//!
//! So the chain walk asks instead: a callable with no own prototype reaches
//! [`super::objects::inherited_from`], and that is where this object is named.
//! `Object.setPrototypeOf(f, p)` still wins, because the own link is asked for
//! first.
//!
//! # Where `bind` keeps what it remembers
//!
//! It is the one of the three that has to *keep* something: a bound function
//! remembers a receiver and a list of arguments. That is a table of **values**
//! beside a cell, and the collector cannot see one — which is why this waited
//! for the question `docs/engine/authoring-natives.md` §8 names.
//!
//! The answer it waited for is that **nothing collects yet**: no caller of
//! `collect::mark` exists, and the accessor table and the array element table
//! are already values an eventual collector has to learn about. So this is the
//! same bet those two make rather than a new one, and it is recorded here as
//! one more table that has to be traced the day there is a tracer.
//!
//! What it is NOT is a property on the callable. A bound function's target and
//! receiver must be unreachable from JavaScript for the same reason a callable's
//! code address is: a program that could write them would choose what the next
//! call jumps to, and with what `this`.
//!
//! # Why `new Function(…)` is not here, and what it needs
//!
//! It answers an ordinary object today, so `new Function("x", "return x")` makes
//! something that is not callable — four suite files die on exactly that, and
//! the diagnostic now says `object is not a function` rather than nothing.
//!
//! It was written against the evaluator the host installs for `node:vm` and
//! REVERTED, because that seam cannot answer this one: `evaluate_source` refuses
//! to hand back a REFERENCE, and a function is one. The reason is not a
//! restriction to lift here — a reference belongs to the region that made it,
//! `compile` makes a region per program, and a function compiled by a second
//! program cannot be called by the first. `compile_graph` is the shape that
//! solves it for modules by emitting everything into ONE compilation, and
//! `new Function` needs the same thing at run time.

use super::{Context, with_current};
use crate::value::Value;
/// `Function`.

#[rtse::class("Function")]
impl Function {

    /// `f.call(thisArg, …)` — any number of arguments.
    ///
    /// # Why the receiver being an argument is the whole difficulty
    ///
    /// The convention carries four values and the receiver spends one of them, so
    /// this method saw three where the caller wrote four: `f.call(o, 1, 2, 3, 4)`
    /// ran `f` with a `4` that had silently become `undefined`. It was not a
    /// refusal at the call site — the site sees five arguments to `call`, which
    /// is one too many for the slots, so it already spills them into the vector
    /// the runtime holds. Nothing read it.
    ///
    /// So this reads it, and the split below is not an optimisation: `built`
    /// allocates a region cell and nothing collects one, so a `.call` in a loop
    /// that took the vector path unconditionally would exhaust the region. Four
    /// or fewer goes straight through, allocating nothing, exactly as before.
    fn call(this: u64, receiver: u64, a: u64, b: u64, c: u64) -> u64 {
        let (arguments, absent) = with_current(|context| {
            (
                super::array_proto::arguments_at(context, 1, [receiver, a, b, c]),
                super::objects::undefined_of(context),
            )
        });
        if arguments.len() > super::functions::ARGUMENT_SLOTS {
            // `built` takes the context itself, so it is reached with no borrow
            // held — and so is the call, whose callee is user code.
            let vector = super::array_proto::built(arguments);
            return super::functions::call_with_args(this, receiver, vector);
        }
        let mut slots = [absent; super::functions::ARGUMENT_SLOTS];
        for (slot, value) in slots.iter_mut().zip(arguments) {
            *slot = value;
        }
        super::functions::call(this, receiver, slots[0], slots[1], slots[2], slots[3])
    }

    /// `f.apply(thisArg, args)`.
    ///
    /// The reason the argument vector had to exist first: this is the spelling
    /// that carries any number of arguments, and it is `call_with_args` from the
    /// other side.
    fn apply(this: u64, receiver: u64, arguments: u64) -> u64 {
        super::functions::call_with_args(this, receiver, arguments)
    }

    /// `f.bind(thisArg, …)` — a new function with the receiver fixed.
    ///
    /// The partial arguments come first at every later call, which is what makes
    /// `f.bind(null, 1)(2)` the same as `f(1, 2)`. Any number of them, read the
    /// way [`Function::call`] reads its own — a binding that quietly dropped its
    /// fourth partial argument would produce the wrong call at every later use of
    /// the bound function rather than at the `bind`.
    fn bind(this: u64, receiver: u64, a: u64, b: u64, c: u64) -> u64 {
        let partial =
            with_current(|context| super::array_proto::arguments_at(context, 1, [receiver, a, b, c]));
        bound(this, receiver, partial)
    }
}

/// What a bound function remembers, beside its cell.
///
/// Not in the cell and not as a property: see the module documentation for why
/// a program able to write either would be choosing what the next call jumps to.
pub(super) struct Bound {
    /// The function the bound one calls.
    target: u64,
    /// The receiver it calls it with, whatever the caller passes.
    receiver: u64,
    /// The arguments that come before the caller's.
    partial: Vec<u64>,
}

impl Context {
    /// What a cell is bound to, if it is a bound function.
    pub(super) fn bound_at(&self, cell: u32) -> Option<&Bound> {
        self.bound.get(cell)
    }
}

/// The bound function itself.
///
/// The partial list arrives with the convention's padding already dropped, so
/// `f.bind(null)` prepends nothing — otherwise every bound function would push
/// three `undefined`s in front of the caller's arguments and no bound call would
/// ever line up.
fn bound(target: u64, receiver: u64, partial: Vec<u64>) -> u64 {
    let made = with_current(|context| {
        let made = super::native::callable(context, forward);
        if let Some(cell) = Value(made).as_slot() {
            context.bound.set(cell, Bound {
                target,
                receiver,
                partial,
            });
        }
        made
    });
    made
}

/// What every bound function runs.
///
/// One native for all of them, because what differs between two bound functions
/// is data rather than code — and a native per binding would be a code address
/// minted per call to `bind`.
extern "C" fn forward(_e: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    // The receiver the caller passed is DISCARDED, which is the whole of what
    // `bind` does: `obj.method()` on a bound function calls it with the bound
    // receiver, not with `obj`.
    let plan = with_current(|context| {
        let cell = Value(current_callee(context)).as_slot()?;
        let held = context.bound_at(cell)?;
        let mut arguments = held.partial.clone();
        // What the CALLER passed, not the four slots: a bound function reached
        // with five arguments has them in the vector, and reading the slots would
        // drop the fifth after `bind` was careful to keep its own.
        arguments.extend(super::array_proto::arguments_at(context, 0, [a0, a1, a2, a3]));
        Some((held.target, held.receiver, arguments))
    });
    let Some((target, receiver, arguments)) = plan else {
        return with_current(|context| super::objects::undefined_of(context));
    };
    let vector = super::array::array_new(arguments.len() as i64);
    with_current(|context| {
        if let Some(cell) = Value(vector).as_slot()
            && let Some(elements) = context.elements_at_mut(cell)
        {
            *elements = arguments;
        }
    });
    // Through the vector path, because the partial arguments plus the caller's
    // can be seven and the convention carries four.
    super::functions::call_with_args(target, receiver, vector)
}

/// Which bound function is running.
///
/// # Why this is a stack and not the environment
///
/// A native's environment slot is `undefined` by construction — `native::callable`
/// closes over nothing — so the one place a bound function could carry its
/// identity is the call itself. `functions::invoke` pushes the callee it is
/// about to jump to, and this reads the top.
fn current_callee(context: &Context) -> u64 {
    context.callees.last().copied().unwrap_or(0)
}

/// What every callable inherits from, made once.
///
/// Lazily, like every other built-in prototype here: a program whose functions
/// are only ever called should not spend the cells for two methods it never
/// reads.
pub(super) fn prototype_of(context: &mut Context) -> Option<u32> {
    if super::class_support::made(context, "Function").is_none() {
        register_function(context);
    }
    Value(super::class_support::prototype(context, "Function")?).as_slot()
}

/// The function currently running, as a value.
///
/// What a NAMED FUNCTION EXPRESSION resolves its own name to. `const f =
/// function fact(n) { … fact(n - 1) … }` binds `fact` for the body and nowhere
/// else, and the value is the function itself rather than the outer `f` — which
/// may be reassigned, or may be a property, or may not exist at all.
///
/// It reads the same stack a bound function reads to know which binding it is,
/// and for the same reason: a compiled function's environment slot holds what it
/// CLOSES OVER, which for one defined at the top level is nothing. The call is
/// where the identity is, so the call is where it is recorded.
///
/// `undefined` when nothing is running, which compiled code cannot reach — the
/// emitter puts this at the top of a body, and a body runs because it was called.
#[rtse::entry]
pub fn running_function() -> u64 {
    with_current(|context| match current_callee(context) {
        0 => super::objects::undefined_of(context),
        callee => callee,
    })
}
