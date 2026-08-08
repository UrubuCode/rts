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

use super::{Context, with_current};
use crate::value::Value;

/// `Function`.
#[rtse::class("Function")]
impl Function {
    /// `f.call(thisArg, a, b, c)`.
    ///
    /// Three arguments rather than four, because the receiver takes one of the
    /// convention's slots. A call with more is refused at the site rather than
    /// losing its arguments here — the same trade `Object.assign` makes, and
    /// `apply` is what a program with an unknown number reaches for.
    fn call(this: u64, receiver: u64, a: u64, b: u64, c: u64) -> u64 {
        // Straight through, with no borrow held: the callee is user code, and
        // its first act may be to call the runtime. `super::functions::call`
        // takes and gives back its own.
        let absent = with_current(|context| super::objects::undefined_of(context));
        super::functions::call(this, receiver, a, b, c, absent)
    }

    /// `f.apply(thisArg, args)`.
    ///
    /// The reason the argument vector had to exist first: this is the spelling
    /// that carries any number of arguments, and it is `call_with_args` from the
    /// other side.
    fn apply(this: u64, receiver: u64, arguments: u64) -> u64 {
        super::functions::call_with_args(this, receiver, arguments)
    }

    /// `f.bind(thisArg, a, b, c)` — a new function with the receiver fixed.
    ///
    /// The partial arguments come first at every later call, which is what makes
    /// `f.bind(null, 1)(2)` the same as `f(1, 2)`. Three of them, because the
    /// receiver takes one of the convention's four slots.
    fn bind(this: u64, receiver: u64, a: u64, b: u64, c: u64) -> u64 {
        bound(this, receiver, [a, b, c])
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
/// Trailing absent arguments are dropped rather than remembered, so
/// `f.bind(null)` prepends nothing — otherwise every bound function would push
/// three `undefined`s in front of the caller's arguments and no bound call would
/// ever line up.
fn bound(target: u64, receiver: u64, partial: [u64; 3]) -> u64 {
    let made = with_current(|context| {
        let absent = super::objects::undefined_of(context);
        let mut partial = partial.to_vec();
        while partial.last() == Some(&absent) {
            partial.pop();
        }
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
        arguments.extend([a0, a1, a2, a3]);
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
