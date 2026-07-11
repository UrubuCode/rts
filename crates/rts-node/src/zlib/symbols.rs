//! node:zlib — the synchronous `extern "C"` entry points. Each takes the input
//! buffer handle, runs the codec, and returns a `Buffer` (throwing an `Error`
//! on a decode failure, `THROWS`-flagged).

use super::codec;
use super::words::{buffer, read_bytes, throw};

/// Run `f` over the arg's bytes → `Buffer`; throw on `Err`.
fn run(handle: u64, f: impl Fn(&[u8]) -> Result<Vec<u8>, String>) -> u64 {
    let bytes = read_bytes(handle);
    match f(&bytes) {
        Ok(out) => buffer(out),
        Err(e) => {
            throw(&e);
            buffer(Vec::new())
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_DEFLATE_SYNC(h: u64) -> u64 {
    run(h, codec::deflate)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_INFLATE_SYNC(h: u64) -> u64 {
    run(h, codec::inflate)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_DEFLATE_RAW_SYNC(h: u64) -> u64 {
    run(h, codec::deflate_raw)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_INFLATE_RAW_SYNC(h: u64) -> u64 {
    run(h, codec::inflate_raw)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_GZIP_SYNC(h: u64) -> u64 {
    run(h, codec::gzip)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_GUNZIP_SYNC(h: u64) -> u64 {
    run(h, codec::gunzip)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_UNZIP_SYNC(h: u64) -> u64 {
    run(h, codec::unzip)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_BROTLI_COMPRESS_SYNC(h: u64) -> u64 {
    run(h, codec::brotli_compress)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_BROTLI_DECOMPRESS_SYNC(h: u64) -> u64 {
    run(h, codec::brotli_decompress)
}

/// `zlib.crc32(data)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_CRC32(h: u64) -> i64 {
    codec::crc32(&read_bytes(h), 0) as i64
}

/// `zlib.crc32(data, value)` — continue from a previous checksum.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_CRC32_PREV(h: u64, prev: i64) -> i64 {
    codec::crc32(&read_bytes(h), prev as u32) as i64
}
