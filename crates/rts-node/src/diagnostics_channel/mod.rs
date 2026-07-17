//! `node:diagnostics_channel` — the in-process publish/subscribe bus, as an
//! ambient `.ts` prelude (Node's own module is ~100% JS over Map/Array; RTS
//! mirrors it over primordials, like `node:stream`). Full core surface:
//!
//! - module functions `channel`/`hasSubscribers`/`subscribe`/`unsubscribe`/
//!   `tracingChannel`;
//! - `class Channel` — `publish`/`subscribe`/`unsubscribe`/`bindStore`/
//!   `unbindStore`/`runStores` + `hasSubscribers` getter + `name`;
//! - `class TracingChannel` — the 5 sub-channels (`start`/`end`/`asyncStart`/
//!   `asyncEnd`/`error`) + `subscribe`/`unsubscribe`/`hasSubscribers` +
//!   `traceSync`/`tracePromise`/`traceCallback`.
//!
//! `unsubscribe(fn)`/`unbindStore(store)` compare by value identity (the handler
//! lives in a `.ts` array). REMOVAL-BY-REFERENCE across a call boundary is
//! currently a no-op match because the engine re-reifies a function value to a
//! FRESH handle on every reference (`f === f` is false) — the same handler passed
//! to `subscribe` then `unsubscribe` arrives as two distinct words. `unsubscribe`
//! still returns a bool and removes nothing; this is an ENGINE-WIDE gap (stable
//! function-value identity), not specific to this module, and blocks
//! `removeListener`-by-ref everywhere. When the engine caches the reified handle
//! per binding, this works with no change here.
//!
//! Deviation (documented in the spec): the named-channel registry is a strong
//! `Map` (Node uses `WeakRef`), and `bindStore`/`runStores` enter a bound
//! store's context only when the store exposes `.run` (real
//! `AsyncLocalStorage` from `node:async_hooks`, not yet implemented) — the sync
//! publish + fn call always run; only cross-async-boundary store propagation is
//! absent. The import names bind to the ambient prelude decls via
//! `node_reexported_globals` (module fns renamed `__dc*`).

use rts_engine::Engine;

/// The ambient `.ts` prelude implementing the whole module. Included once by the
/// engine (see `PRELUDE_TS`), after the timers/events preludes it may build on.
pub const DIAGNOSTICS_CHANNEL_TS: &str = include_str!("diagnostics_channel.ts");

/// Registers the `node:diagnostics_channel` module specifier. The class +
/// function surface is ambient prelude, bound to imports by the engine's flatten
/// pass — no native members here.
pub fn register(e: &mut Engine) {
    e.ns("node:diagnostics_channel")
        .doc(
            "In-process pub/sub bus (node:diagnostics_channel): channel/ \
             hasSubscribers/subscribe/unsubscribe/tracingChannel + Channel/ \
             TracingChannel classes (ambient .ts).",
        )
        .done();
}
