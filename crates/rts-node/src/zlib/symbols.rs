//! node:zlib — the synchronous `extern "C"` entry points. Each takes the input
//! buffer handle, runs the codec, and returns a `Buffer` (throwing an `Error`
//! on a decode failure, `THROWS`-flagged).

use rts_engine::abi::ty::Handle;

use super::codec;
use super::words::{buffer, opt_level, read_bytes, throw};

/// Run a level-parameterized codec over the arg's bytes → `Buffer`.
fn run_level(handle: u64, options: u64, f: impl Fn(&[u8], u32) -> Result<Vec<u8>, String>) -> u64 {
    let bytes = read_bytes(handle);
    match f(&bytes, opt_level(options)) {
        Ok(out) => buffer(out),
        Err(e) => {
            throw(&e);
            buffer(Vec::new())
        }
    }
}

/// `zlib.gzipSync(buffer, options)`.
#[rtse::function(module = "node:zlib", value = "gzipSync", overload = "level", throws)]
fn gzip_sync_level(h: Handle, options: Handle) -> Handle {
    run_level(h, options, codec::gzip_level)
}

/// `zlib.deflateSync(buffer, options)`.
#[rtse::function(module = "node:zlib", value = "deflateSync", overload = "level", throws)]
fn deflate_sync_level(h: Handle, options: Handle) -> Handle {
    run_level(h, options, codec::deflate_level)
}

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

/// `zlib.deflateSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "deflateSync", throws)]
fn deflate_sync(h: Handle) -> Handle {
    run(h, codec::deflate)
}

/// `zlib.inflateSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "inflateSync", throws)]
fn inflate_sync(h: Handle) -> Handle {
    run(h, codec::inflate)
}

/// `zlib.deflateRawSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "deflateRawSync", throws)]
fn deflate_raw_sync(h: Handle) -> Handle {
    run(h, codec::deflate_raw)
}

/// `zlib.inflateRawSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "inflateRawSync", throws)]
fn inflate_raw_sync(h: Handle) -> Handle {
    run(h, codec::inflate_raw)
}

/// `zlib.gzipSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "gzipSync", throws)]
fn gzip_sync(h: Handle) -> Handle {
    run(h, codec::gzip)
}

/// `zlib.gunzipSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "gunzipSync", throws)]
fn gunzip_sync(h: Handle) -> Handle {
    run(h, codec::gunzip)
}

/// `zlib.unzipSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "unzipSync", throws)]
fn unzip_sync(h: Handle) -> Handle {
    run(h, codec::unzip)
}

/// `zlib.brotliCompressSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "brotliCompressSync", throws)]
fn brotli_compress_sync(h: Handle) -> Handle {
    run(h, codec::brotli_compress)
}

/// `zlib.brotliDecompressSync(buffer)`.
#[rtse::function(module = "node:zlib", value = "brotliDecompressSync", throws)]
fn brotli_decompress_sync(h: Handle) -> Handle {
    run(h, codec::brotli_decompress)
}

/// `zlib.crc32(data)`.
#[rtse::function(module = "node:zlib", value = "crc32")]
fn crc32(h: Handle) -> i64 {
    codec::crc32(&read_bytes(h), 0) as i64
}

/// `zlib.crc32(data, value)` — continue from a previous checksum.
#[rtse::function(module = "node:zlib", value = "crc32", overload = "prev")]
fn crc32_prev(h: Handle, prev: i64) -> i64 {
    codec::crc32(&read_bytes(h), prev as u32) as i64
}
