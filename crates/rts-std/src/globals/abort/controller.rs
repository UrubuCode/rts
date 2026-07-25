//! `AbortController` — `new AbortController()` + `.signal` + `.abort(reason?)`.
//! Parity source: `rts-shared/src/stdlib/events.ts`'s `class AbortController`.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::alloc_rtse;

use super::signal::{AbortSignal, do_abort};

#[rtse::class("AbortController")]
#[derive(Clone, Default)]
pub struct AbortController {
    signal: u64,
}

#[rtse::class("AbortController")]
impl AbortController {
    /// `new AbortController()` — allocates its own (never-aborted) `AbortSignal`.
    #[rtse::ctor]
    fn new() -> Self {
        let s = alloc_rtse("AbortSignal", AbortSignal::default());
        AbortController { signal: s }
    }

    // `returns = "AbortSignal"` names the return class so `c.signal.aborted` /
    // `c.signal.addEventListener(...)` classify the handle as an AbortSignal and
    // resolve its (inherited) members — without it the handle rides as ts `object`
    // and the chained reads return `undefined`.
    #[rtse::getter(returns = "AbortSignal")]
    fn signal(self: &AbortController) -> Handle {
        self.signal
    }

    /// `controller.abort(reason?)` — aborts `.signal` (default reason is a
    /// real `DOMException("This operation was aborted", "AbortError")`, see
    /// `abort/mod.rs`'s doc comment).
    #[rtse::method(optional = 1)]
    fn abort(self: &mut AbortController, reason: Handle) {
        do_abort(self.signal, reason, "AbortError", "This operation was aborted");
    }
}
