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

use std::cell::Cell;

use super::functions::{Spelling, invoke};
use super::objects::undefined_of;
use super::with_current;
use crate::value::Value;

thread_local! {
    /// Whether a tail call is recorded: the discriminant of the slot below.
    ///
    /// # Why this is not a field of `Context`, where it first lived
    ///
    /// Because every door asks it after EVERY call, and the answer is almost
    /// always no. As an `Option<TailCall>` inside `Context`, the question cost
    /// a move of the whole record inside the door's borrow, and the eighty
    /// bytes it added to `Context` moved the fields the call path reads. On
    /// 2026-09-18 that measured +8–12 % per call through the runtime
    /// (`bench/analytic.ts`, `call 0 args` 21.8 → 24.4 ns, release, three
    /// interleaved runs against the tree without it). Here the question is
    /// one thread-local load, the same move `current.rs` made for the throw
    /// in flight.
    ///
    /// It is not a flag beside a real slot: it IS the slot's `Option`
    /// discriminant, and [`take`] derives the `Option` from it — one source,
    /// like `InFlight::live`.
    static LIVE: Cell<bool> = const { Cell::new(false) };
    /// The recorded call, meaningful only while [`LIVE`].
    static CALL: Cell<TailCall> = const { Cell::new(TailCall::NONE) };
}

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
    /// The contents of an empty slot; never read while it is empty.
    const NONE: Self = TailCall {
        callee: 0,
        this: 0,
        count: 0,
        name: 0,
        arguments: [0; super::functions::ARGUMENT_SLOTS],
    };

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
    // Two records at once would mean an activation returned without its door
    // settling — the one invariant the whole scheme stands on.
    debug_assert!(!LIVE.with(Cell::get), "an unsettled tail call");
    CALL.with(|slot| {
        slot.set(TailCall {
            callee,
            this,
            count: count.clamp(0, slots) as usize,
            name,
            arguments: [a0, a1, a2, a3],
        })
    });
    LIVE.with(|live| live.set(true));
    with_current(|context| undefined_of(context))
}

/// The recorded call, if any, emptying the slot. One load when there is none.
#[inline]
pub(super) fn take() -> Option<TailCall> {
    if !LIVE.with(Cell::get) {
        return None;
    }
    LIVE.with(|live| live.set(false));
    Some(CALL.with(Cell::get))
}

/// Every word of a recorded call that may be a reference, for the collector.
pub(super) fn pending_words() -> Option<TailCall> {
    LIVE.with(Cell::get).then(|| CALL.with(Cell::get))
}

/// What an activation that just returned `produced` REALLY answered.
///
/// `produced` itself when it made no tail call; otherwise the answer of the
/// call it recorded, and of the one THAT recorded, for as long as the chain
/// runs — iteratively, so its length costs no stack. Every door calls this
/// after popping what it pushed, and the common answer is one load.
#[inline]
pub(super) fn settle(produced: u64) -> u64 {
    match take() {
        None => produced,
        Some(next) => settle_chain(next),
    }
}

/// The chain itself, out of line so the door's common path stays a load and a
/// branch.
///
/// Each call pushes the argument vector and count an ordinary call pushes, so
/// the callee reads its own `arguments` and not the finished activation's.
#[cold]
#[inline(never)]
fn settle_chain(first: TailCall) -> u64 {
    let mut pending = Some(first);
    let mut produced = 0;
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
        with_current(|context| {
            context.pending_arguments.pop();
            context.pending_counts.pop();
        });
        pending = take();
    }
    produced
}
