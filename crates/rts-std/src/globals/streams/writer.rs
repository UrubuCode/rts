//! `WritableStreamDefaultWriter` — parity source: `streams.ts`'s
//! `class WSDWriter`. Holds the `WritableStream`'s own handle (assigned by
//! `writable::__RTS_FN_GL_WRITABLESTREAM_GET_WRITER`, a hand-written residual
//! — see `writable.rs`) so `write`/`close` mutate the SAME stream instance.

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::poly::POLY_UNDEFINED;

#[rtse::class("WritableStreamDefaultWriter")]
#[derive(Clone, Default)]
pub struct WritableStreamDefaultWriter {
    pub(crate) stream: Handle,
}

#[rtse::class("WritableStreamDefaultWriter")]
impl WritableStreamDefaultWriter {
    #[rtse::method(name = "write")]
    fn write(self: &WritableStreamDefaultWriter, chunk: Poly) -> Poly {
        super::writable::write_into(self.stream, chunk);
        POLY_UNDEFINED
    }

    #[rtse::method(name = "close")]
    fn close(self: &WritableStreamDefaultWriter) -> Poly {
        super::writable::close_of(self.stream);
        POLY_UNDEFINED
    }

    #[rtse::method(name = "abort", optional = 1)]
    fn abort(self: &WritableStreamDefaultWriter, _reason: Poly) -> Poly {
        POLY_UNDEFINED
    }

    #[rtse::method(name = "releaseLock")]
    fn release_lock(self: &WritableStreamDefaultWriter) {}
}
