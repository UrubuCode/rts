//! `TransformStream` — parity source: `streams.ts`'s `class TransformStream`.
//! Wires a fresh `readable`+`writable` pair sharing ONE controller, with the
//! writable side in `mode::TRANSFORM` (the transformer object, if any, drives
//! `write()`/`close()` via `transform`/`flush`).

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::handles::alloc_rtse;

#[rtse::class("TransformStream")]
#[derive(Clone, Default)]
pub struct TransformStream {
    pub(crate) readable: Handle,
    pub(crate) writable: Handle,
}

#[rtse::class("TransformStream")]
impl TransformStream {
    #[rtse::ctor(optional = 1)]
    fn new(transformer: Poly) -> Self {
        let ctl = alloc_rtse(
            "ReadableStreamDefaultController",
            super::controller::ReadableStreamDefaultController::default(),
        );
        let readable = alloc_rtse("ReadableStream", super::readable::ReadableStream { ctl, locked: false });
        let t = if super::is_nullish(transformer) { 0 } else { transformer };
        let writable = super::writable::new_linked(ctl, t, super::writable::mode::TRANSFORM, false);
        TransformStream { readable, writable }
    }

    #[rtse::getter(returns = "ReadableStream")]
    fn readable(self: &TransformStream) -> Handle {
        self.readable
    }

    #[rtse::getter(returns = "WritableStream")]
    fn writable(self: &TransformStream) -> Handle {
        self.writable
    }
}
