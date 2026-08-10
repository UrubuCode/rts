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

use std::cell::RefCell;

use super::Context;

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
    let value = body();
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
