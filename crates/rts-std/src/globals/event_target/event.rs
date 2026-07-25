//! `Event` — `new Event(type, opts?)` with `.type`/`.defaultPrevented`/
//! `.cancelable`/`.bubbles`/`.target`/`.currentTarget` + `preventDefault()`/
//! `stopPropagation()`/`stopImmediatePropagation()`. Parity source:
//! `rts-shared/src/stdlib/events.ts`'s `class Event`.
//!
//! `type` can't be a Rust field name (keyword) — stored as `kind`, exposed via
//! `#[rtse::getter(name = "type")]`. `target`/`currentTarget` are plain `u64`
//! handles (any-typed, default `null` — `0` boxes to `null` per
//! `box_object_handle`, matching the `.ts`'s `target: any = null` default);
//! they are stamped by `EventTarget::dispatch_on` (a different file/module),
//! so the accessors below are `pub(crate)` plain (non-macro) helpers, not
//! JS-exposed setters — tighter than the `.ts` (whose fields are technically
//! externally assignable, though nothing does so).

use rts_engine::abi::ty::Handle;

use super::opt_flag;

#[rtse::class("Event")]
#[derive(Clone, Default)]
pub struct Event {
    kind: String,
    #[rtse::variable(readonly)]
    default_prevented: bool,
    #[rtse::variable(readonly)]
    cancelable: bool,
    #[rtse::variable(readonly)]
    bubbles: bool,
    target: u64,
    current_target: u64,
}

/// Plain (non-`#[rtse::*]`) helpers — internal construction/mutation used by
/// `EventTarget`/`AbortSignal` dispatch, not part of the JS-visible surface.
impl Event {
    /// Build an `abort`-style internal event (no options bag) — used by
    /// `AbortSignal::do_abort`.
    pub(crate) fn new_internal(kind: &str) -> Self {
        Event {
            kind: kind.to_string(),
            ..Default::default()
        }
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn is_default_prevented(&self) -> bool {
        self.default_prevented
    }

    pub(crate) fn set_target(&mut self, h: u64) {
        self.target = h;
    }

    pub(crate) fn set_current_target(&mut self, h: u64) {
        self.current_target = h;
    }
}

#[rtse::class("Event")]
impl Event {
    /// `new Event(type, opts?)` — `opts` is `{cancelable?, bubbles?}` (same
    /// options-bag reading as `EventTarget.addEventListener`'s `{once}`).
    #[rtse::ctor(optional = 1)]
    fn new(ty: &str, opts: Handle) -> Self {
        Event {
            kind: ty.to_string(),
            default_prevented: false,
            cancelable: opt_flag(opts, "cancelable"),
            bubbles: opt_flag(opts, "bubbles"),
            target: 0,
            current_target: 0,
        }
    }

    #[rtse::getter(name = "type")]
    fn js_type(self: &Event) -> String {
        self.kind.clone()
    }

    #[rtse::getter]
    fn target(self: &Event) -> Handle {
        self.target
    }

    #[rtse::getter(name = "currentTarget")]
    fn current_target_get(self: &Event) -> Handle {
        self.current_target
    }

    /// `defaultPrevented` flips to `true` only when `cancelable` — matches the
    /// `.ts`'s `if (this.cancelable) { this.defaultPrevented = true; }`.
    #[rtse::method(name = "preventDefault")]
    fn prevent_default(self: &mut Event) {
        if self.cancelable {
            self.default_prevented = true;
        }
    }

    #[rtse::method(name = "stopPropagation")]
    fn stop_propagation(self: &Event) {}

    #[rtse::method(name = "stopImmediatePropagation")]
    fn stop_immediate_propagation(self: &Event) {}
}
