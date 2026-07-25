//! `ReadableStreamDefaultReader` — parity source: `streams.ts`'s
//! `class RSDReader`. Stores the controller handle directly (not "its
//! stream"): `read()` only ever needs the queue, so there is no need for
//! `getReader()` to know its own stream's handle either (see `readable.rs`).

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::poly::POLY_UNDEFINED;

#[rtse::class("ReadableStreamDefaultReader")]
#[derive(Clone, Default)]
pub struct ReadableStreamDefaultReader {
    pub(crate) ctl: Handle,
}

#[rtse::class("ReadableStreamDefaultReader")]
impl ReadableStreamDefaultReader {
    #[rtse::method(name = "read")]
    fn read(self: &ReadableStreamDefaultReader) -> Handle {
        match super::controller::shift(self.ctl) {
            Some(v) => super::make_result(v, false),
            None => super::make_result(POLY_UNDEFINED, true),
        }
    }

    #[rtse::method(name = "cancel", optional = 1)]
    fn cancel(self: &ReadableStreamDefaultReader, _reason: Poly) -> Poly {
        POLY_UNDEFINED
    }

    #[rtse::method(name = "releaseLock")]
    fn release_lock(self: &ReadableStreamDefaultReader) {}
}
