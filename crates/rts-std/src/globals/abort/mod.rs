//! `AbortController` + `AbortSignal extends EventTarget` (#62) — DRAIN_MOTOR
//! §11 (owner 2026-07-24 correction): reimplemented as `#[rtse::class]` at
//! FULL PARITY with the live `.ts` polyfill (`rts-shared/src/stdlib/
//! events.ts`), which is the authoritative semantics source. `AbortSignal`
//! EMBEDS an `EventTarget` (`base`) and FORWARDS
//! addEventListener/removeEventListener (composition + forwarding — the
//! `Entry::Rtse` downcast model has no parent-method-on-child dispatch);
//! `extends = "EventTarget"` links the Registry parent so `signal instanceof
//! EventTarget` resolves (mirrors `point3.rs`'s `RtsePoint3 extends
//! RtsePoint` proof).
//!
//! One `#[rtse::class]` per file: `signal.rs` (`AbortSignal`), `controller.rs`
//! (`AbortController`).
//!
//! **Gap CLOSED** (DRAIN_MOTOR §11 bonus, 2026-07-24): the SPEC default
//! abort/timeout reason is `new DOMException(msg, name)` — now a REAL
//! `DOMException` instance. `DOMException` moved from an ambient per-program
//! `.ts` class to a `#[rtse::class]` (`rts-shared/src/globals/dom_exception`),
//! which is a normal compiled Rust symbol (`__rtsm_global_domexception_new`) —
//! [`default_abort_reason`] below calls `rts_shared::globals::dom_exception::
//! new_dom_exception` directly (an ordinary Cargo dependency call, `rts-std`
//! already depends on `rts-shared`), so `signal.reason instanceof
//! DOMException` + `.code` now hold. An EXPLICIT reason passed by the caller
//! (`signal.abort(myReason)`) is untouched — this only affects the
//! synthesized DEFAULT reason.
pub mod controller;
pub mod signal;

pub use controller::register as register_abort_controller_class_spec;
pub use signal::register_abort_signal_class_spec;

/// Build the default reason for a no-arg `abort()`/`timeout()` — a real
/// `DOMException(message, name)` instance (see the module doc). Returns a raw
/// HandleTable handle (matching the `Handle`-typed `reason` field/getter).
pub(crate) fn default_abort_reason(name: &str, message: &str) -> u64 {
    rts_shared::globals::dom_exception::new_dom_exception(message, name)
}
