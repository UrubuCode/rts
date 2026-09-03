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
//! inventory and all of `entry/modules.rs`) for a store stack, an id counter,
//! and a JS-visible `throw`: neither of those two exists.
//!
//! **The third thing this search used to report missing — a callable carrying
//! captured state — does exist, and this doc previously said otherwise.**
//! [`rts_core::entry::closure_new`] takes a code address AND an environment
//! and delivers the environment back as the callee's own first argument;
//! `perf_hooks::timerify` and `util::promisify` were already built on it when
//! this line still said "no native here mints a closure". `bind` and
//! `snapshot`, below, are what re-running the search against that function
//! rather than `make_callable` (which really is a bare pointer, and really has
//! none) produced. Searched this crate for the argument-shift helper Node's
//! overloads need and found `crate::fs::options_and_listener`, which is
//! `pub(crate)` — nothing here needs the shift (no member below is
//! `fn(x[, options], cb)`), so it is named rather than copied.
//!
//! # What decides whether a member is built here
//!
//! One question: does the answer depend on a call site this crate owns?
//!
//! `AsyncLocalStorage` and `AsyncResource` scope things *synchronously* — push,
//! call, pop — and every part of that is Rust plus
//! [`rts_core::entry::call`]. They are built, `bind`/`snapshot` included: a
//! bound function or a snapshot runner is the identical push/call/pop, just
//! wrapped in a closure so the push can happen LATER, at a call this crate
//! still owns, rather than immediately.
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
//! `scope[Symbol.dispose]` — the host surface names
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
//! stated rather than silently truncated. The `bind`/`snapshot` family does
//! NOT share this limit: what they capture (the target, the resource, a
//! receiver, a whole frame stack) lives in the closure's ENVIRONMENT rather
//! than an argument slot, so all four real slots stay free for the call the
//! wrapper actually forwards.
//!
//! # GC
//!
//! Every frame stack here holds live values in a Rust `Vec` the collector's
//! stack scan cannot see (reference doc §5.4). Nothing on the host surface
//! registers an extra root source, so a store held only by a frame is at risk
//! across a collection. Reported rather than worked around: a shadow copy on
//! some reachable object would be a second answer to what the store is.
//!
//! `local::static_bind`/`static_snapshot` clone [`local`]'s stack into that
//! same unrooted `Vec` before copying it into a real JS array — so the read
//! is exposed to exactly the window every other reader of that stack already
//! is, no better and no worse. Once built, the array is different: it is an
//! ordinary reachable value hanging off the closure's environment, so a
//! CAPTURED snapshot is, if anything, safer across a collection for the rest
//! of its life than the live stack it was copied from.

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
///
/// Also the `prototype.constructor` back-link, via
/// `entry::declare_host_class` — `crate::stream::class_ctor`'s doc has the
/// mechanism: a hand-built class's constructor is a bare `make_callable`,
/// which `closure_new` (the compiled-class path) never runs, so nothing else
/// ever wrote it and `new AsyncResource().constructor.name` read `"Object"`.
fn attach(
    context: &mut Context,
    namespace: u64,
    name: &'static str,
    methods: &[(&str, Provided)],
    arity: u32,
) -> u64 {
    let prototype = rts_core::entry::make_prototype(context, name, methods);
    let constructor = rts_core::entry::get_member(context, namespace, name);
    rts_core::entry::put_member(context, constructor, "prototype", prototype);
    rts_core::entry::declare_host_class(context, constructor, prototype, name, arity);
    prototype
}
