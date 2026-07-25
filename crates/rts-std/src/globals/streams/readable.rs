//! `ReadableStream` — parity source: `streams.ts`'s `class ReadableStream`.
//!
//! `getReader()`/`pipeThrough()` only ever need `self.ctl` (already available
//! in the macro's cloned receiver) — neither needs "this stream's own
//! handle", so both are plain `#[rtse::method]`s (unlike `WritableStream`'s
//! `getWriter`, see `writable.rs`).

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::handles::alloc_rtse;
use rts_engine::heap::shapes::handle_word_auto;

#[rtse::class("ReadableStream")]
#[derive(Clone, Default)]
pub struct ReadableStream {
    pub(crate) ctl: Handle,
    #[rtse::variable]
    pub(crate) locked: bool,
}

#[rtse::class("ReadableStream")]
impl ReadableStream {
    /// `new ReadableStream(source?)` — allocates a fresh controller and, if
    /// `source` duck-types a `start` method, calls `source.start(controller)`
    /// synchronously (matches `.ts`'s `if ("start" in source)`).
    #[rtse::ctor(optional = 1)]
    fn new(source: Poly) -> Self {
        let ctl = alloc_rtse(
            "ReadableStreamDefaultController",
            super::controller::ReadableStreamDefaultController::default(),
        );
        if !super::is_nullish(source) {
            super::call_if_present(source, "start", &[handle_word_auto(ctl)]);
        }
        ReadableStream { ctl, locked: false }
    }

    #[rtse::method(name = "getReader", returns = "ReadableStreamDefaultReader")]
    fn get_reader(self: &ReadableStream) -> Handle {
        alloc_rtse(
            "ReadableStreamDefaultReader",
            super::reader::ReadableStreamDefaultReader { ctl: self.ctl },
        )
    }

    /// Drains what's already queued into `pair.writable`, then arms lazy
    /// forwarding for everything enqueued from now on (`.ts`'s lazy-pipe
    /// model — see `controller::pipe_into`). Returns `pair.readable`.
    #[rtse::method(name = "pipeThrough")]
    fn pipe_through(self: &ReadableStream, pair: Poly) -> Poly {
        let writable_w = super::get_prop(pair, "writable");
        let readable_w = super::get_prop(pair, "readable");
        let sink_h = super::handle_of(writable_w);
        super::controller::pipe_into(self.ctl, sink_h);
        readable_w
    }
}
