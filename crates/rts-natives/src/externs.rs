//! The ONE declaration site for symbols whose bodies live ABOVE this crate and
//! are bound by the LINKER.
//!
//! Six symbols, and every one of them is a layer above doing something only it
//! can do: invoking a JS function value, constructing a primordial-class
//! instance, settling a Promise, resuming an async state machine. A lower layer
//! that needs one of those has no dependency edge to reach it — but it does not
//! need one, because the JIT registers the symbol in its table and the AOT
//! staticlib exports it, so a forward declaration is enough for either to bind.
//!
//! ## Why this file exists at all, and why it is HERE
//!
//! It exists because the alternative was measured and was worse: before a single
//! declaration site, ~15 crates each hand-rolled their own copy of these
//! `extern "C"` blocks and they **drifted** — `fn` vs `pub safe fn`, partial
//! subsets — tripping `clashing_extern_declarations`. One declaration, one
//! signature.
//!
//! It is HERE, at the bottom of the graph, and no longer in `rts-engine`, and
//! that move is the point of `RTS_ORGANIZATION.md` N2b. The old file was
//! `rts-engine/src/gc_surface.rs`, and the plan called it "an inverted
//! dependency wearing a forward-declaration as a disguise" — correctly, because
//! most of what it declared was GC machinery that had escaped upward into
//! `rts-std`. N1/N2/N3 brought that machinery home, and the declarations for it
//! became ordinary `pub use`s of real items, which is why the callers now name
//! `heap::string_pool` and `collector::error` directly and three shim files
//! (`rts-primitives`, `rts-shared`, `rts-std` each had a `gc_surface.rs` with
//! zero functions of its own) are deleted.
//!
//! What remains never escaped. `Function.prototype.call` belongs in
//! `rts-primitives` because `Function` is a primordial class; `Promise` settling
//! belongs in `rts-std` because it needs the scheduler. A low layer producing a
//! JS value of a class defined above it is a real cross-layer need, not a
//! misplacement — so this is a link contract, not an inversion, and it is named
//! for what it is.
//!
//! `pub safe fn` (edition 2024): the real definitions are plain Rust `extern
//! "C"` functions with no caller invariants to uphold, so declaring them `safe`
//! preserves the call-site ergonomics (no `unsafe` block needed).

unsafe extern "C" {
    /// Generic runtime-dispatch invocation of a Function-class handle:
    /// `callee(this_arg, ...args)`. (`rts-primitives` `function::ops`)
    pub safe fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;

    /// `Function.prototype.call`-style invocation of a Function-class handle.
    /// (`rts-primitives` `function::ops`)
    ///
    /// Used from inside this crate too: the eager iteration protocol has to
    /// invoke a USER iterator object's own `next`.
    pub safe fn __RTS_FN_GL_FUNCTION_CALL(handle: u64, this_arg: i64, args_handle: u64) -> i64;

    /// Construct a `RangeError` instance, returning its handle.
    /// (`rts-primitives` `error::instance`)
    ///
    /// Used from inside this crate: the recursion-depth guard raises
    /// "Maximum call stack size exceeded".
    pub safe fn __RTS_FN_GL_RANGE_ERROR_NEW(msg_ptr: i64, msg_len: i64, cause: u64) -> u64;

    /// Create an already-fulfilled Promise wrapping `value`, return its handle.
    /// (`rts-std` `globals::fetch::instance`)
    pub safe fn __RTS_FN_GL_PROMISE_RESOLVE(value: u64) -> i64;

    /// Create an already-rejected Promise wrapping `reason`, return its handle.
    /// (`rts-std` `globals::fetch::instance`)
    pub safe fn __RTS_FN_GL_PROMISE_REJECT(reason: u64) -> i64;

    /// Inject a settled value/error into an async state machine and run its next
    /// step. (`rts-std` `collector::generator` — the async DRIVER half, which
    /// stayed up there because it schedules: timers, tokio, microtasks.)
    pub safe fn __rtsn_async_sm_resume(h: u64, value: i64, rejected: i64);

    /// Invoke a JS FUNCTION VALUE (a `PolyValue` word tagged FUNCTION) with four
    /// `PolyValue` args plus a rest array, returning a `PolyValue` word.
    /// Codegen-owned (`rts-runtime` `adapters::value::funcops`).
    ///
    /// This is what lets a backend (EventEmitter, Proxy, a socket pump) call
    /// back into a stored callback without transmuting the word as a code
    /// pointer, which crashed. JIT-only for now — the symbol is not in the AOT
    /// archive.
    pub safe fn __rtsadp_fn_invoke(fn_word: u64, a0: u64, a1: u64, a2: u64, a3: u64, rest: u64)
    -> u64;
}
