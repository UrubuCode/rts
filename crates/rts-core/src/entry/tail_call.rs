//! A call in tail position, run AFTER the activation that wrote it is gone.
//!
//! # What the language asks for
//!
//! ECMAScript 2015 made a call in tail position of STRICT code a proper tail
//! call: `PrepareForTailCall` discards the running execution context before
//! the callee is entered, so `function loop(n) { return n ? loop(n - 1) : 0 }`
//! runs in constant stack at any depth. JavaScriptCore does it, which is why
//! Bun answers `loop(500000)`; V8 never shipped it, which is why Node answers
//! `RangeError: Maximum call stack size exceeded` for the same program.
//!
//! # Why a trampoline and not the machine's `return_call`
//!
//! `rts-cranelift` has a real tail call — `Terminator::TailCall`, lowered to
//! Cranelift's `return_call` under `Convention::InternalTail`. It needs a
//! callee the COMPILER knows, and a JavaScript callee is a value: every call
//! here crosses into [`super::functions`], which reads the heap to find out
//! whether the value is code. Compiled JavaScript never calls compiled
//! JavaScript directly, so there is no compiled-to-compiled frame for the
//! machine to replace — the growth is `invoke`'s Rust frames as much as the
//! compiled one, and only this side can remove those.
//!
//! So the compiler emits [`tail_call`] where it would have emitted a call,
//! and returns whatever that answers. This records the callee and its operands
//! and answers `undefined` without calling anything. The compiled function
//! then returns, its frame and the door's frames unwind, and [`settle`] — run
//! by every door AFTER it has popped what it pushed for the finished
//! activation — finds the record and makes the call from there. A chain of
//! tail calls is a loop in `settle`, one level deep, however long it runs.
//!
//! # Where the compiler may emit it, and where it must not
//!
//! Decided in `rts-codegen`'s `emit/tail.rs`, and the reason is the one thing
//! this file relies on: nothing may run between the record and the return. A
//! `finally`, a protected region, an iterator's close, a generator's or async
//! body's resumption — each is work the specification does AFTER the callee
//! answers, and each is a place the compiler refuses to write one.
//!
//! # `new.target`, and why the doors settle rather than `invoke`
//!
//! A constructor reached through `new` may end in `return f()`. The target
//! stack says which activation `new` named by DEPTH, and the tail callee runs
//! at the depth the constructor ran at — so settling inside `invoke`, before
//! `construct_inner` popped its target, would answer the constructor's
//! `new.target` to a function that was CALLED. Settling in the door, after its
//! own pops, is what makes the callee's activation an ordinary call.

use super::functions::{Spelling, invoke};
use super::objects::undefined_of;
use super::with_current;
use crate::value::Value;

/// A tail call recorded and not yet made.
///
/// The operands of `call_counted`, kept verbatim: what the call would have
/// been, had it been made where it was written.
#[derive(Clone, Copy)]
pub struct TailCall {
    callee: u64,
    this: u64,
    count: usize,
    name: i64,
    arguments: [u64; super::functions::ARGUMENT_SLOTS],
}

impl TailCall {
    /// Every word of this record that may be a reference, for the collector.
    ///
    /// `count` and `name` are numbers the compiler wrote, never values, so
    /// offering them would ask the root filter whether a small integer is a
    /// reference — the question `roots` refuses for `new_targets`' depth.
    pub fn words(&self) -> impl Iterator<Item = u64> + '_ {
        [self.callee, self.this].into_iter().chain(self.arguments)
    }
}

/// `return f(a, b)` in tail position: records the call and answers `undefined`.
///
/// The answer is never read as the function's result — [`settle`] replaces it
/// with what the recorded call produces. It is `undefined` rather than some
/// marker because a marker is a value a program could be handed if a door ever
/// forgot to settle, and `undefined` is the least wrong thing it could see.
///
/// Cannot raise: it calls nothing and reads nothing a program wrote, which is
/// what puts it on `rts-codegen`'s `CANNOT_RAISE`.
#[rtse::entry]
pub fn tail_call(
    callee: u64,
    this: u64,
    count: i64,
    name: i64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
) -> u64 {
    let slots = super::functions::ARGUMENT_SLOTS as i64;
    with_current(|context| {
        // Two records at once would mean an activation returned without
        // its door settling — the one invariant the whole scheme stands on.
        debug_assert!(context.pending_tail.is_none(), "an unsettled tail call");
        context.pending_tail = Some(TailCall {
            callee,
            this,
            count: count.clamp(0, slots) as usize,
            name,
            arguments: [a0, a1, a2, a3],
        });
        undefined_of(context)
    })
}

/// What an activation that just returned `produced` REALLY answered.
///
/// `produced` itself when it made no tail call; otherwise the answer of the
/// call it recorded, and of the one THAT recorded, for as long as the chain
/// runs — iteratively, so its length costs no stack.
pub(super) fn settle(produced: u64) -> u64 {
    let pending = with_current(|context| context.pending_tail.take());
    settle_taken(produced, pending)
}

/// [`settle`], for a door that already took the record inside the borrow it
/// pops its own stacks in — which is `called` and `call_with_args`, the two
/// every call goes through, so that an activation making no tail call pays a
/// move inside a borrow it was taking anyway rather than a borrow of its own.
///
/// Each call pushes the argument vector and count an ordinary call pushes, so
/// the callee reads its own `arguments` and not the finished activation's.
pub(super) fn settle_taken(mut produced: u64, mut pending: Option<TailCall>) -> u64 {
    while let Some(next) = pending {
        // The one refusal `called` makes before its jump, made here for the
        // same reason: `return C()` of a class is the `TypeError` it would be
        // anywhere else, and not a constructor run without `new`.
        let refused = with_current(|context| {
            let class = Value(next.callee)
                .as_slot()
                .is_some_and(|cell| context.is_class_constructor(cell));
            if !class {
                let absent = undefined_of(context);
                context.pending_arguments.push(absent);
                context.pending_counts.push(Some(next.count));
            }
            class
        });
        if refused {
            super::throw::type_error("Class constructor cannot be invoked without 'new'");
            return with_current(|context| undefined_of(context));
        }
        let [a0, a1, a2, a3] = next.arguments;
        let spelling = Spelling::Literal(next.name);
        produced = invoke(next.callee, next.this, spelling, a0, a1, a2, a3);
        pending = with_current(|context| {
            context.pending_arguments.pop();
            context.pending_counts.pop();
            context.pending_tail.take()
        });
    }
    produced
}
