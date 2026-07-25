//! `CompressionStream` — gzip/deflate. NOT part of the current `.ts` (the
//! prelude never had it); ported instead from the git-history hand-written
//! Rust (`git show 7e4868d8^:crates/rts-std/src/globals/readable_stream/
//! instance.rs`, the last commit before the drain-campaign sweep deleted the
//! whole family). A `readable`+`writable` pair whose writable side is
//! `mode::COMPRESS`: raw bytes accumulate as each chunk is written, and
//! `writer.close()` gzip/zlib-compresses the accumulated bytes into a
//! `Buffer` enqueued on the readable side — same behavior as the old code.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::alloc_rtse;

#[rtse::class("CompressionStream")]
#[derive(Clone, Default)]
pub struct CompressionStream {
    pub(crate) readable: Handle,
    pub(crate) writable: Handle,
}

#[rtse::class("CompressionStream")]
impl CompressionStream {
    /// `new CompressionStream(format)` — `format` is `"gzip"` or `"deflate"`
    /// (anything else defaults to gzip, matching the old Rust).
    #[rtse::ctor]
    fn new(format: &str) -> Self {
        let ctl = alloc_rtse(
            "ReadableStreamDefaultController",
            super::controller::ReadableStreamDefaultController::default(),
        );
        let readable = alloc_rtse("ReadableStream", super::readable::ReadableStream { ctl, locked: false });
        let deflate = format == "deflate";
        let writable = super::writable::new_linked(ctl, 0, super::writable::mode::COMPRESS, deflate);
        CompressionStream { readable, writable }
    }

    #[rtse::getter(returns = "ReadableStream")]
    fn readable(self: &CompressionStream) -> Handle {
        self.readable
    }

    #[rtse::getter(returns = "WritableStream")]
    fn writable(self: &CompressionStream) -> Handle {
        self.writable
    }
}

/// gzip-compress `data` (default compression level) — matches the old
/// hand-written Rust's `gzip_bytes`.
pub(crate) fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// zlib/deflate-compress `data` — matches the old hand-written Rust's
/// `deflate_bytes`.
pub(crate) fn deflate_bytes(data: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}
