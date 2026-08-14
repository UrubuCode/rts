//! Reaching this thread's context.
//!
//! # Why an entry point cannot receive it
//!
//! The boundary is `extern "C"` over ABI types — `u64`, `i64`, `i32`, `f64`,
//! `bool` and strings. A `&mut Context` does not cross that and never will, so
//! an operation that needs the heap reaches ambient state instead of being
//! handed it.
//!
//! The alternative that was rejected is threading a context pointer through
//! every call site: it works, it costs a register and an argument everywhere,
//! and it lets a caller pass the wrong one.
//!
//! # One per thread, not one per process
//!
//! A global behind a lock would serialise every property read in the program,
//! which is the opposite of what a per-region heap is for.
//!
//! # What else a second thread would have to be given, and the answer
//!
//! Nothing. That was checked rather than assumed, because "the context is
//! thread-local" only makes a thread independent if the context is the *only*
//! mutable state a running program reaches. It is: this crate declares no
//! process-global mutable state at all, and the three entry points the machine
//! dials without being asked — `alloc`, `cache_resolve`, `write_barrier` —
//! reach the heap through [`with_current`] like everything else.
//!
//! `rts-cranelift` has two `OnceLock`s, and both are in `target`: a thread pool
//! and its size, used while **placing** code. Neither is reachable from a
//! running program.
//!
//! So a thread that installs a context over its own region shares nothing that
//! can change. What it must still be given, per thread and not once, is the two
//! seeded tables — the key registry and the literal table — because their
//! contents are cells in *that* thread's region.

use std::cell::{Cell, RefCell};

use super::Context;

thread_local! {
    /// The throw in flight, as `(tag, payload)`.
    ///
    /// # Why this is not a field of [`Context`], where it used to live
    ///
    /// Because compiled code asks after **every** call that can raise, and the
    /// asking was the expensive half. Reaching it through [`with_current`] costs
    /// a thread-local lookup, a `RefCell` borrow (a write, a check and a
    /// restore) and an index into a `Vec`, to read one word that is almost
    /// always `None`. Here it is a thread-local lookup and a load.
    ///
    /// Worth 3–6 % on a loop whose body is one array element read, and nothing
    /// where the runtime is not called — `super::throw::thrown` carries the
    /// table and the reason the raw subtraction overstates it.
    ///
    /// It is not a *mirror* of the field — the field is gone. A cached flag
    /// beside the real slot would be a second source of one fact, kept in step
    /// by hand, which is what this repository's "one source, generated views"
    /// rule is about: two answers to one question is how the pair drifts and a
    /// caught throw starts looking uncaught.
    ///
    /// One slot and not a stack, for the reason it always was: a second throw
    /// cannot start before the first is caught or ends the program, because the
    /// only thing that runs in between is compiled code returning.
    ///
    /// Nesting is handled by [`with_context`], which saves this across an inner
    /// program and restores it afterwards — the same lifetime the field had when
    /// it was part of the context being pushed.
    static THROWN: Cell<InFlight> = const { Cell::new(InFlight::NONE) };
}

/// The throw in flight, in a layout whose first word compiled code can LOAD.
///
/// # Why the shape is spelled out rather than left to `Option`
///
/// Because the address of the first word is now part of an agreement:
/// `super::throw::thrown_address` hands it to compiled code, which reads it
/// after every operation that can raise instead of calling to ask. `Option`'s
/// discriminant has no guaranteed position — `repr(C)` here is what makes
/// "offset zero is whether something is in flight" a fact rather than an
/// observation about today's layout.
///
/// It is still ONE source. `live` is not a cached copy of a truth held
/// elsewhere: it IS the discriminant, and `slot()` derives the `Option` from
/// it rather than the other way round. A flag beside a real slot is the drift
/// this module already refused once and is refusing again.
#[derive(Clone, Copy)]
#[repr(C)]
struct InFlight {
    /// Non-zero when a throw is in flight. Offset zero, by `repr(C)`.
    live: i64,
    /// What kind of throw, meaningful only while `live`.
    tag: i64,
    /// The value raised, meaningful only while `live`.
    payload: u64,
}

impl InFlight {
    /// Nothing in flight.
    const NONE: Self = InFlight {
        live: 0,
        tag: 0,
        payload: 0,
    };

    /// The pair the rest of the crate speaks in.
    fn slot(self) -> Option<(i64, u64)> {
        match self.live {
            0 => None,
            _ => Some((self.tag, self.payload)),
        }
    }
}

/// The throw in flight, without taking it.
pub(crate) fn thrown_slot() -> Option<(i64, u64)> {
    THROWN.with(|slot| slot.get().slot())
}

/// Whether a throw is in flight — the whole of what compiled code asks after a
/// call, and the reason [`THROWN`] is not reached through [`with_current`].
pub(crate) fn thrown_pending() -> bool {
    THROWN.with(|slot| slot.get().live != 0)
}

/// Where this thread's flag word lives.
///
/// # Why an address rather than a constant
///
/// Compiled code reads this word after every operation that can raise, and a
/// call to ask was most of what the read cost. Two ways of avoiding the call
/// were available and both are wrong: an address baked in while compiling is
/// wrong for an object file, which runs in a different process from the one
/// that compiled it, and a data symbol of the module is shared between threads
/// while this word is not. Handing the address out at run time answers both,
/// because it is asked on the thread that will read it.
///
/// The value is stable for the life of the thread, which is what makes asking
/// once per activation enough.
pub(crate) fn thrown_address() -> u64 {
    THROWN.with(|slot| slot as *const Cell<InFlight> as u64)
}

/// Puts a throw in flight, replacing whatever was there.
pub(crate) fn set_thrown(value: Option<(i64, u64)>) {
    let held = match value {
        Some((tag, payload)) => InFlight {
            live: 1,
            tag,
            payload,
        },
        None => InFlight::NONE,
    };
    THROWN.with(|slot| slot.set(held));
}

/// Takes the throw in flight, clearing it.
pub(crate) fn take_thrown_slot() -> Option<(i64, u64)> {
    THROWN.with(|slot| slot.replace(InFlight::NONE).slot())
}

thread_local! {
    /// This thread's contexts, innermost last, empty until something installs
    /// one.
    ///
    /// # Why a stack and not one slot
    ///
    /// It was one slot, and installing over it **dropped what was there**. That
    /// is fine for a host running one program and fatal the moment a running
    /// program evaluates source: `node:vm` and `node:repl` reach
    /// [`super::evaluate`], the host compiles a second program, installing it
    /// destroyed the caller's heap, and the first entry point after the inner
    /// program returned found no context and aborted. Both modules were written
    /// and left unregistered for exactly this.
    ///
    /// The rejected alternative was making the evaluator refuse re-entry. It is
    /// cheaper and it makes `vm.runInNewContext` answer `undefined` from inside
    /// a program, which is where a program actually calls it — so it removes the
    /// abort by removing the feature. A stack is also the shape a second context
    /// on another thread needs, so it is the one that pays twice.
    ///
    /// Nesting does NOT make the inner program's references usable outside it: a
    /// reference belongs to the region that made it, and that rule is unchanged.
    /// What the stack fixes is the outer heap surviving.
    static CONTEXTS: RefCell<Vec<Context>> = const { RefCell::new(Vec::new()) };
}

/// Install a context for this thread, and run something with it.
///
/// Returns the context afterwards so a caller can inspect what a program left
/// behind — which is how the tests below work, and how a host would read a
/// result out.
///
/// Nesting is allowed: the context installed here is the current one until this
/// call returns, and whatever was installed before it becomes current again.
pub fn with_context<T>(context: Context, body: impl FnOnce() -> T) -> (Context, T) {
    CONTEXTS.with(|stack| stack.borrow_mut().push(context));
    // The throw in flight belongs to the program that raised it, and an inner
    // program starts with none. Saving it here is what the slot got for free
    // while it was a field of the context being pushed; making it thread-local
    // is what makes reading it cheap, so the lifetime is restored by hand.
    let outer = take_thrown_slot();
    let value = body();
    set_thrown(outer);
    let context = CONTEXTS.with(|stack| stack.borrow_mut().pop());
    (
        context.expect("the context pushed above is still on the stack"),
        value,
    )
}

/// Run something against this thread's context.
///
/// Aborts when there is none. That is not a runtime condition a program can
/// reach — it means compiled code ran before anything installed a heap, which is
/// a broken embedding — and unwinding out of an `extern "C"` frame is undefined
/// behaviour, so there is nothing better to do than say so and stop.
pub(crate) fn with_current<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    CONTEXTS.with(|stack| {
        let mut borrowed = stack.borrow_mut();
        let Some(context) = borrowed.last_mut() else {
            eprintln!("rts: an entry point ran with no context installed on this thread");
            std::process::abort();
        };
        body(context)
    })
}
