//! `node:async_hooks` — context propagation built, async instrumentation built
//! only as far as this crate owns a call site.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search, re-run for this pass over
//! the members added here. Searched `rts-cranelift`'s `src/sched/`
//! (`SchedulerId`, `Delivery`, `ContinuationId`) and `src/frame/` for anything
//! that carries a VALUE across a suspension point: nothing does — those decide
//! *when* a continuation runs, which is a different concern, so the frame
//! stacks in [`local`] and [`resource`] are new state rather than a second copy
//! of something. Searched `rts_core::entry` (`entry/mod.rs`'s `Context`
//! inventory and all of `entry/modules.rs`) for a store stack, an id counter, a
//! callable carrying captured state, and a JS-visible `throw`: none of the four
//! exists. Searched this crate for the argument-shift helper Node's overloads
//! need and found `crate::fs::options_and_listener`, which is `pub(crate)` —
//! nothing here needs the shift (no member below is `fn(x[, options], cb)`), so
//! it is named rather than copied. `perf_hooks`, `events`, `domain` and
//! `fs/mod.rs` all name the same missing capability this module hits again: a
//! native callable has no environment slot, so a native cannot mint a closure.
//!
//! # What decides whether a member is built here
//!
//! One question: does the answer depend on a call site this crate owns?
//!
//! `AsyncLocalStorage` and `AsyncResource` scope things *synchronously* — push,
//! call, pop — and every part of that is Rust plus
//! [`rts_core::entry::call`]. They are built.
//!
//! `createHook`'s callbacks are supposed to fire around **every** async
//! resource in the process: every timer tick, every promise settle, every
//! socket callback. Those call sites live in the timer and promise machinery,
//! which has no hook point and is not this crate's. So `createHook` is built
//! for exactly the resources this module itself creates — an explicit
//! `new AsyncResource(...)`, its `runInAsyncScope`, its `emitDestroy` — and
//! reports nothing for anything else. That is the phase the reference doc
//! (§5.8i) describes as best-effort, and the alternative rejected here was
//! shipping `createHook` as a no-op registration: a hook that registers
//! successfully and never fires is a wrong answer that runs, because a tracing
//! tool reads "no async activity" from it.
//!
//! # The divergence a program must know about
//!
//! Context does **not** survive `await`, `setTimeout`, or any other
//! suspension: nothing snapshots the frame stacks at scheduling time and
//! restores them before the callback, because the scheduling sites are not
//! here (reference doc §5.7). Everything below is correct for synchronous
//! nesting and loses context across an async boundary — the "context loss"
//! failure real Node documents, triggered by an RTS-specific boundary.
//! Likewise `executionAsyncId()` reports `0` anywhere outside a
//! `runInAsyncScope`, including inside a promise or timer callback where Node
//! reports that resource's own id.
//!
//! # Not implemented, by name
//!
//! `AsyncLocalStorage.bind` and `AsyncLocalStorage.snapshot` (static),
//! `asyncResource.bind` and `AsyncResource.bind` (static) — each must hand
//! back a NEW callable carrying a captured context, and
//! [`rts_core::entry::make_callable`] takes a bare `extern "C" fn` with no
//! environment slot to hold one; the four modules named under Reuse-check
//! above hit the same wall. `scope[Symbol.dispose]` — the host surface names
//! properties with strings ([`rts_core::entry::put_member`]) and mints no
//! symbol key, so `using scope = als.withScope(x)` cannot be made to work;
//! `withScope` answers a `RunScope` with a plain `dispose()`, which
//! is the fallback the reference doc's §5.8f names. `promiseResolve` and
//! `trackPromises` in `createHook(options)` — accepted and never fired,
//! because the point `resolve()` runs is inside the promise machinery; a
//! program passing one gets a stderr line saying so rather than silence.
//! `asyncWrapProviders` — Node's map of its own internal C++ provider names
//! (`TCPWRAP`, `FSREQCALLBACK`, …) to ids; RTS has no such taxonomy and
//! inventing a plausible one is exactly the fabricated value this crate
//! refuses. `AsyncHook` as a name on the namespace — Node does not export the
//! class either, only `createHook`. The `TypeError` `createHook` throws for a
//! non-function hook field, and the error a second `emitDestroy()` throws:
//! nothing here can raise a catchable JS exception
//! ([`rts_core::entry::throw`] ends the process), so both report to stderr
//! and continue, the same trade `node:assert` states for a failed assertion.
//! `requireManualDestroy` and GC-triggered auto-`destroy` — there is no
//! finalization callback on this surface, so `destroy` fires only from an
//! explicit `emitDestroy()`, which makes `requireManualDestroy: true` already
//! the only behaviour and the option therefore meaningless rather than
//! honoured. Arguments past the second are dropped by `run`, `exit` and
//! `runInAsyncScope`: a native has four argument slots, spent on the receiver
//! shift plus the callback, and Node's `...args` tails have nowhere to go —
//! stated rather than silently truncated.
//!
//! # GC
//!
//! Every frame stack here holds live values in a Rust `Vec` the collector's
//! stack scan cannot see (reference doc §5.4). Nothing on the host surface
//! registers an extra root source, so a store held only by a frame is at risk
//! across a collection. Reported rather than worked around: a shadow copy on
//! some reachable object would be a second answer to what the store is.

mod hooks;
mod local;
mod resource;

use rts_core::entry::{Context, Provided};

/// The namespace `node:async_hooks` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("AsyncLocalStorage", local::construct),
        ("AsyncResource", resource::construct),
        ("createHook", hooks::create_hook),
        ("executionAsyncId", resource::execution_async_id),
        ("triggerAsyncId", resource::trigger_async_id),
        ("executionAsyncResource", resource::execution_async_resource),
    ];
    let namespace = rts_core::entry::make_namespace(context, members);
    local::install(context, namespace);
    resource::install(context, namespace);
    namespace
}

/// Hangs a class's prototype off the constructor already on a namespace.
///
/// Three classes here need the identical four lines, and the one thing that
/// must not vary between them is that the prototype installed on the
/// constructor is the SAME object a `new` links an instance to — which is what
/// `make_prototype`'s name-keyed identity gives, and what a second call
/// building its own object would quietly break for `instanceof`.
fn attach(
    context: &mut Context,
    namespace: u64,
    name: &'static str,
    methods: &[(&str, Provided)],
) -> u64 {
    let prototype = rts_core::entry::make_prototype(context, name, methods);
    let constructor = rts_core::entry::get_member(context, namespace, name);
    rts_core::entry::put_member(context, constructor, "prototype", prototype);
    prototype
}
