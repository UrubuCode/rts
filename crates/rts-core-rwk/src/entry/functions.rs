//! Callables: making one, and calling one.
//!
//! # Why a call is an entry point at all
//!
//! The machine has `call_indirect`, and a compiler that knew the callee's
//! address could use it directly. It does not know: a JavaScript callee is a
//! *value*, and finding out whether that value is code — and which code — reads
//! the heap. That is the membership rule, unmodified.
//!
//! There is a second reason, and it is the one that decided the shape. A value
//! that is **not** callable must not be jumped to, and the language says what
//! happens instead: `1()` throws a `TypeError`. Throwing needs the machine's
//! protected regions and nothing emits those yet, so the check has to live
//! somewhere that can fail without them. Compiled code cannot; this can.
//!
//! # What a callable is
//!
//! A region cell at a reserved layout, holding two words: where the code is,
//! and the environment it closed over. Not an object with two properties —
//! `code` would then be a key in the registry, readable and **writable** from
//! JavaScript, and a program that stored a number there would name the
//! instruction the next call jumps to.
//!
//! # The fixed arity, stated where it is paid
//!
//! Every compiled function has one shape:
//!
//! ```text
//! extern "C" fn(env, this, a0, a1, a2, a3) -> value
//! ```
//!
//! Four argument slots, missing ones filled with `undefined` by the caller.
//! JavaScript's arity is dynamic and this is not, which is a real restriction
//! and a named one: the compiler refuses a call with more arguments rather than
//! dropping them here, because a call whose fifth argument silently vanished is
//! a wrong program that runs.
//!
//! The alternative — a count and a pointer to a vector of arguments — is what a
//! real engine does and what this becomes. It needs somewhere to put the
//! vector, and a caller-allocated one is a stack slot this compiler does not
//! emit yet. Fixing the arity buys the whole of E5 without it, and the shape
//! above is what changes when that arrives.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::value::Value;

/// How many argument slots a compiled function has.
///
/// Public because it is a **contract**, not a detail: the compiler emits
/// functions with exactly this many argument parameters and pads calls to it,
/// and the two agreeing is what makes the call below anything other than
/// undefined behaviour. A constant read by one side and remembered by the other
/// is the disagreement this crate keeps naming.
pub const ARGUMENT_SLOTS: usize = 4;

/// The shape every compiled JavaScript function has.
///
/// Written as a type alias so the `transmute` below names it once. Two spellings
/// of this, one at the definition and one at the call, is how an argument comes
/// to be read as the wrong thing — and a wrong signature here is not a wrong
/// answer, it is a jump with a corrupt stack.
type Compiled = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// Makes a callable out of a code address and an environment.
///
/// The code address comes from the machine's `FuncAddr`, which is a relocation
/// the destination filled in — so it is a real address by the time this runs,
/// and this never computes one.
#[rtse::entry]
pub fn closure_new(code: i64, environment: u64) -> u64 {
    with_current(|context| {
        // An ordinary object, because a function IS one: `f.x = 1` works, and
        // `f.prototype` will. What makes it callable is recorded beside the
        // cell, where nothing a program can write reaches it.
        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
        match context.region.alloc(crate::heap::STRIDE, ty) {
            Some(cell) => {
                context.mark_callable(cell, code as u64, environment);
                // Every function gets a `prototype` object, because `new`
                // reads one and a function that could not be constructed with
                // would be a different kind of function. Made here rather than
                // on demand: `F.prototype.m = …` before any `new F()` is the
                // ordinary way to write a method, so it has to exist first.
                let shape = context.shapes.root();
                let ty = context.layout_of(shape).index() as u32;
                if let Some(prototype) = context.region.alloc(crate::heap::STRIDE, ty) {
                    let key = prototype_key(context);
                    super::objects::put(context, cell, key, Value::from_slot(prototype).bits());
                }
                Value::from_slot(cell).bits()
            }
            // The region is full and there is no collector to ask. Answering
            // `undefined` is wrong — the language makes a function here — and
            // it is less wrong than handing back cell zero, which is a real
            // object belonging to somebody else. The same answer `object_new`
            // gives, for the same reason.
            None => undefined_of(context),
        }
    })
}

/// Calls a value, with a receiver and up to [`ARGUMENT_SLOTS`] arguments.
///
/// Answers `undefined` for a callee that is not callable, where the language
/// throws a `TypeError`. A stated gap and the same one property access has:
/// throwing needs protected regions and nothing emits those yet. It is named
/// here rather than left implicit because *this* gap is the one that would
/// otherwise be a jump to an arbitrary address.
#[rtse::entry]
pub fn call(callee: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    // Read what is needed, then LET GO of the context before jumping.
    //
    // Not a tidiness: `with_current` holds a `RefCell` borrow for as long as its
    // body runs, and the callee is compiled code whose very first act may be to
    // call `__rts_add`. Calling from inside the borrow panics on the re-entry —
    // a deadlock this repository has already paid for once, in a different
    // shard, and the reason this function is written as two statements rather
    // than one expression.
    let found = with_current(|context| resolve(context, callee));

    let Some((code, environment)) = found else {
        return with_current(|context| undefined_of(context));
    };

    // SAFETY: `code` was written by `closure_new` from a `FuncAddr`, which the
    // machine emits only for a function this compilation declared, and every
    // compiled JavaScript function is placed with the signature `Compiled`
    // spells. The cell it came from is at the closure layout, which nothing
    // else allocates and no JavaScript can write to — that check is what
    // `resolve` is for, and it is why the address cannot be a value a program
    // chose.
    let entry: Compiled = unsafe { std::mem::transmute::<u64, Compiled>(code) };
    entry(environment, this, a0, a1, a2, a3)
}

/// The code and environment of a value, when it is genuinely a callable.
///
/// The type check is the whole safety argument for the `transmute` above, which
/// is why it reads the cell's header rather than trusting the tag: a reference
/// says "a cell", not "a callable", and every object in the region answers the
/// same tag.
fn resolve(context: &mut Context, callee: u64) -> Option<(u64, u64)> {
    let slot = Value(callee).as_slot()?;
    context.callable_at(slot)
}

/// The key `prototype` has.
fn prototype_key(context: &mut Context) -> crate::object::Key {
    context.well_known("prototype")
}

/// `new f(…)`.
///
/// # What the operation actually is
///
/// Three steps the language keeps separate and a caller cannot: make an object
/// whose prototype is the callee's `prototype` property, run the callee with
/// that object as `this`, and answer the object — **unless** the callee
/// returned one of its own, in which case that wins.
///
/// The last clause is the one an implementation forgets. `function F() {
/// return {a: 1}; }` produces the returned object and not the fresh one, and a
/// factory written that way is ordinary JavaScript rather than a corner.
///
/// # Why the fresh object is made here and not by the compiler
///
/// Because its prototype comes from a value — `f.prototype` — that only exists
/// while running. A compiler could emit the allocation, and then it would have
/// to emit the property read and the link, which is three entry points where
/// this is one.
#[rtse::entry]
pub fn construct(callee: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let prepared = with_current(|context| {
        let cell = Value(callee).as_slot()?;
        context.callable_at(cell)?;

        // `f.prototype`, read as an ordinary property — which is what it is,
        // now that a function is an object.
        let key = prototype_key(context);
        let prototype = super::objects::read_property(context, cell, key)?;

        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
        let fresh = context.region.alloc(crate::heap::STRIDE, ty)?;
        context.set_prototype(fresh, prototype.bits());
        Some(Value::from_slot(fresh).bits())
    });

    let Some(this) = prepared else {
        // Not callable, or the region is full. `new 1` is a `TypeError`, and
        // throwing needs protected regions — the same stated gap calling has.
        return with_current(|context| undefined_of(context));
    };

    let produced = call(callee, this, a0, a1, a2, a3);
    // A constructor that returned an object produced THAT. Anything else — a
    // number, `undefined`, the usual — leaves the fresh object as the answer.
    if Value(produced).as_slot().is_some() {
        produced
    } else {
        this
    }
}

/// `value instanceof callee`.
///
/// Walks what `value` inherits from, looking for the callee's `prototype`
/// **object** — not the callee. `x instanceof F` is false for an `x` whose
/// chain never reaches `F.prototype`, and true for one that reaches it however
/// many links away, which is why this is a loop rather than one comparison.
#[rtse::entry]
pub fn instance_of(value: u64, callee: u64) -> bool {
    with_current(|context| {
        let Some(function) = Value(callee).as_slot() else {
            return false;
        };
        if context.callable_at(function).is_none() {
            // `1 instanceof 2` is a `TypeError`. Answering false is the same
            // stated gap every other operation has while throwing is missing.
            return false;
        }
        let key = prototype_key(context);
        let Some(wanted) = super::objects::read_property(context, function, key) else {
            return false;
        };
        let Some(mut cell) = Value(value).as_slot() else {
            return false;
        };
        for _ in 0..super::objects::CHAIN_LIMIT {
            let Some(next) = context.prototype_at(cell) else {
                return false;
            };
            if next == wanted.bits() {
                return true;
            }
            let Some(further) = Value(next).as_slot() else {
                return false;
            };
            cell = further;
        }
        false
    })
}
