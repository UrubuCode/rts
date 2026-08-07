//! `node:zlib` — the synchronous compress/decompress surface, buffer to
//! buffer.
//!
//! # Reuse check (`reuse-check` skill, run before writing anything here)
//!
//! `rts-cranelift` has nothing to say about a codec: compression is heap-and-
//! bytes work, not a machine capability, so no search there answers this.
//! Inside this crate, `crates/rts-node/src/zlib/` is a DIFFERENT crate — the
//! old engine's `node:zlib`, over `AbiType`/`StrPtr`/a symbol table that
//! `rts-node-rwk` does not use and must not reach into. Nothing in
//! `rts-node-rwk` itself answers zlib/gzip/deflate/brotli; `docs/reference/
//! node/zlib.md` is the spec, written against the OLD engine's ABI
//! (`__RTS_FN_NODE_ZLIB_*`, `StrPtr options_json`, a private handle table) —
//! its §5.2 ABI table and §5.8 phase plan do not apply to this crate, which
//! has no `extern "C"` symbol table of its own and reads arguments through
//! `rts_core_rwk::entry::modules` instead. What DOES transfer from that spec:
//! the crate choices (§5.1 — `flate2` with the `rust_backend` feature, the
//! `brotli` crate, both already vetted and already in this crate's
//! `Cargo.toml`), the option defaults (§3), and the test plan (§6).
//!
//! # Buffers, not a second byte container
//!
//! Every function here reads its input with `entry::bytes_of` and answers
//! with `entry::make_buffer` — the real `Buffer` class `rts-core-rwk` builds,
//! not a `Uint8Array` this module would have to fabricate parity for. A
//! caller's `Buffer.isBuffer(zlib.gzipSync(x))` is `true`.
//!
//! # Streaming classes — REFUSED
//!
//! `Gzip`/`Gunzip`/`Deflate`/`Inflate`/`DeflateRaw`/`InflateRaw`/`Unzip`/
//! `BrotliCompress`/`BrotliDecompress` and their `createX()` factories are
//! NOT built here, even though `node:stream`'s `Transform` is real in this
//! crate (`crate::stream`). The mechanism refused, specifically: `flate2`'s
//! incremental `Compress`/`Decompress` and `brotli`'s `CompressorWriter` both
//! need to be OWNED across calls — one `_transform` call feeds them a chunk
//! and a LATER, separate call drains more output or calls `.flush()`/
//! `.close()`. That is state living longer than one native call, and this
//! crate is `#![deny(dead_code)]` with no handle table of its own to hold it
//! in (the old engine's spec, §5.2, gives itself exactly this — a private
//! sharded handle table — which is the ABI surface this crate does not have).
//! Building one FOR this module alone, rather than as infrastructure every
//! handle-bearing `rts-node-rwk` module could share, is the second table this
//! crate's own doctrine (`lib.rs`, `reuse-check`) refuses. So: refused by
//! name, not half-built silently as a Transform that only ever sees one
//! chunk.
//!
//! # No throw
//!
//! Like every other module in this crate (see `crypto::kdf`'s doc), there is
//! no throw available through `rts_core_rwk::entry::modules`. A decode
//! failure (truncated/corrupt input) answers an EMPTY `Buffer` rather than
//! Node's thrown `Error` — named here rather than silently matching Node's
//! shape.
//!
//! # Not implemented, by name
//!
//! Every callback (non-`Sync`) convenience function (`gzip`, `gunzip`,
//! `deflate`, `inflate`, `deflateRaw`, `inflateRaw`, `unzip`,
//! `brotliCompress`, `brotliDecompress`) — no event loop or `spawn_blocking`
//! bridge is reachable from this crate (the same offload gap
//! `crypto::random`'s doc names). The `Zstd*` classes and
//! `zstdCompress(Sync)`/`zstdDecompress(Sync)` — Node marks this whole
//! sub-surface Experimental itself, and it needs a crate decision
//! (`docs/reference/node/zlib.md` §7) this module does not make. `zlib.crc32`
//! — a plausible next addition (`crc32fast` is not yet a dependency of this
//! crate) but out of scope for the synchronous compress/decompress surface
//! this pass builds. `info: true`'s `{ buffer, engine }` result shape — there
//! is no live `engine` instance to hand back without the streaming classes.
//! The `dictionary` option (Deflate/Inflate family) and every other
//! `ZlibOptions`/`BrotliOptions` tuning knob (`level`, `memLevel`,
//! `strategy`, `windowBits`, `chunkSize`, `flush`, `finishFlush`,
//! `maxOutputLength`) — reading an options object is possible through
//! `entry::get_member`, but every function here already uses its one
//! available argument slot for the input buffer, so there is nowhere left to
//! put it without exceeding the four-argument-max convention on a function
//! that only needs two JS-visible parameters (`buffer`, `options`). Deferred
//! rather than half-read.

use flate2::Compression;
use flate2::read::{GzDecoder, ZlibDecoder};
use flate2::write::{DeflateDecoder, DeflateEncoder, GzEncoder, ZlibEncoder};
use std::io::{Read, Write};

use rts_core_rwk::entry::{self, Context, Provided};

/// The namespace `node:zlib` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("gzipSync", gzip_sync),
        ("gunzipSync", gunzip_sync),
        ("deflateSync", deflate_sync),
        ("inflateSync", inflate_sync),
        ("deflateRawSync", deflate_raw_sync),
        ("inflateRawSync", inflate_raw_sync),
        ("brotliCompressSync", brotli_compress_sync),
        ("brotliDecompressSync", brotli_decompress_sync),
        ("unzipSync", unzip_sync),
    ];
    let namespace = entry::make_namespace(context, members);
    let constants = constants_table(context);
    entry::put_member(context, namespace, "constants", constants);
    namespace
}

/// `zlib.constants` — the subset a caller reading `zlib.constants.Z_*` needs
/// to pass a recognizable value through, even though this pass does not read
/// any of them back out of an options object yet (see the module doc).
fn constants_table(context: &mut Context) -> u64 {
    let object = entry::make_object(context);
    let entries: &[(&str, f64)] = &[
        ("Z_NO_FLUSH", 0.0),
        ("Z_PARTIAL_FLUSH", 1.0),
        ("Z_SYNC_FLUSH", 2.0),
        ("Z_FULL_FLUSH", 3.0),
        ("Z_FINISH", 4.0),
        ("Z_BLOCK", 5.0),
        ("Z_OK", 0.0),
        ("Z_STREAM_END", 1.0),
        ("Z_NEED_DICT", 2.0),
        ("Z_ERRNO", -1.0),
        ("Z_STREAM_ERROR", -2.0),
        ("Z_DATA_ERROR", -3.0),
        ("Z_MEM_ERROR", -4.0),
        ("Z_BUF_ERROR", -5.0),
        ("Z_VERSION_ERROR", -6.0),
        ("Z_NO_COMPRESSION", 0.0),
        ("Z_BEST_SPEED", 1.0),
        ("Z_BEST_COMPRESSION", 9.0),
        ("Z_DEFAULT_COMPRESSION", -1.0),
        ("Z_FILTERED", 1.0),
        ("Z_HUFFMAN_ONLY", 2.0),
        ("Z_RLE", 3.0),
        ("Z_FIXED", 4.0),
        ("Z_DEFAULT_STRATEGY", 0.0),
        ("BROTLI_OPERATION_PROCESS", 0.0),
        ("BROTLI_OPERATION_FLUSH", 1.0),
        ("BROTLI_OPERATION_FINISH", 2.0),
        ("BROTLI_OPERATION_EMIT_METADATA", 3.0),
        ("BROTLI_PARAM_MODE", 0.0),
        ("BROTLI_MODE_GENERIC", 0.0),
        ("BROTLI_MODE_TEXT", 1.0),
        ("BROTLI_MODE_FONT", 2.0),
        ("BROTLI_PARAM_QUALITY", 1.0),
        ("BROTLI_MIN_QUALITY", 0.0),
        ("BROTLI_MAX_QUALITY", 11.0),
        ("BROTLI_DEFAULT_QUALITY", 11.0),
        ("BROTLI_PARAM_LGWIN", 2.0),
        ("BROTLI_MIN_WINDOW_BITS", 10.0),
        ("BROTLI_MAX_WINDOW_BITS", 24.0),
        ("BROTLI_DEFAULT_WINDOW", 22.0),
        ("BROTLI_LARGE_MAX_WINDOW_BITS", 30.0),
        ("BROTLI_PARAM_LGBLOCK", 3.0),
    ];
    for (name, value) in entries {
        let held = entry::make_number(*value);
        entry::put_member(context, object, name, held);
    }
    object
}

/// The input buffer's bytes, from a context already in hand — `None` for a
/// value that is not a `Buffer`/`TypedArray`/`DataView`/`ArrayBuffer`, in
/// which case every caller here answers `undefined` rather than throwing.
///
/// `undefined` and NOT an empty `Buffer`, which the first version answered: an
/// empty buffer is a legitimate result — `gunzipSync` of the gzip of `""` IS
/// empty — so answering it for a failure makes the two indistinguishable. That
/// is the same convention `node:fs` states for a read that failed, and the same
/// reason.
fn input_bytes(context: &Context, value: u64) -> Vec<u8> {
    entry::bytes_of(context, value)
        .or_else(|| entry::text_in(context, value).map(String::into_bytes))
        .unwrap_or_default()
}

/// `zlib.gzipSync(buffer)`.
extern "C" fn gzip_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        let out = match encoder.write_all(&input).and_then(|()| encoder.finish()) {
            Ok(bytes) => bytes,
            Err(_) => return entry::undefined_in(context),
        };
        entry::make_buffer(context, &out)
    })
}

/// `zlib.gunzipSync(buffer)`.
extern "C" fn gunzip_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut decoder = GzDecoder::new(&input[..]);
        let mut out = Vec::new();
        // The read is CHECKED: a decoder over bytes that are not what it expects
        // fails, and answering the empty vector it filled would be
        // indistinguishable from decompressing an empty input.
        if decoder.read_to_end(&mut out).is_err() {
            return entry::undefined_in(context);
        }
        entry::make_buffer(context, &out)
    })
}

/// `zlib.deflateSync(buffer)` — zlib-wrapped DEFLATE (RFC 1950).
extern "C" fn deflate_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        let out = match encoder.write_all(&input).and_then(|()| encoder.finish()) {
            Ok(bytes) => bytes,
            Err(_) => return entry::undefined_in(context),
        };
        entry::make_buffer(context, &out)
    })
}

/// `zlib.inflateSync(buffer)`.
extern "C" fn inflate_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut decoder = ZlibDecoder::new(&input[..]);
        let mut out = Vec::new();
        // The read is CHECKED: a decoder over bytes that are not what it expects
        // fails, and answering the empty vector it filled would be
        // indistinguishable from decompressing an empty input.
        if decoder.read_to_end(&mut out).is_err() {
            return entry::undefined_in(context);
        }
        entry::make_buffer(context, &out)
    })
}

/// `zlib.deflateRawSync(buffer)` — raw DEFLATE, no zlib header/trailer
/// (RFC 1951).
extern "C" fn deflate_raw_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        let out = match encoder.write_all(&input).and_then(|()| encoder.finish()) {
            Ok(bytes) => bytes,
            Err(_) => return entry::undefined_in(context),
        };
        entry::make_buffer(context, &out)
    })
}

/// `zlib.inflateRawSync(buffer)`.
extern "C" fn inflate_raw_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut decoder = DeflateDecoder::new(Vec::new());
        let out = match decoder.write_all(&input).and_then(|()| decoder.finish()) {
            Ok(bytes) => bytes,
            Err(_) => return entry::undefined_in(context),
        };
        entry::make_buffer(context, &out)
    })
}

/// `zlib.brotliCompressSync(buffer)` — default quality/window (see the
/// module doc: `BrotliOptions.params` is not read yet).
extern "C" fn brotli_compress_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut out = Vec::new();
        let params = brotli::enc::BrotliEncoderParams::default();
        let mut reader = &input[..];
        let _ = brotli::BrotliCompress(&mut reader, &mut out, &params);
        entry::make_buffer(context, &out)
    })
}

/// `zlib.brotliDecompressSync(buffer)`.
extern "C" fn brotli_decompress_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut out = Vec::new();
        let mut reader = &input[..];
        let _ = brotli::BrotliDecompress(&mut reader, &mut out);
        entry::make_buffer(context, &out)
    })
}

/// `zlib.unzipSync(buffer)` — auto-detects gzip (`1f 8b` magic) vs
/// zlib-wrapped deflate by header sniff, per `docs/reference/node/zlib.md`
/// §5.1.
extern "C" fn unzip_sync(_e: u64, _this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let input = input_bytes(context, buffer);
        let mut out = Vec::new();
        if input.starts_with(&[0x1f, 0x8b]) {
            let mut decoder = GzDecoder::new(&input[..]);
            let _ = decoder.read_to_end(&mut out);
        } else {
            let mut decoder = ZlibDecoder::new(&input[..]);
            let _ = decoder.read_to_end(&mut out);
        }
        entry::make_buffer(context, &out)
    })
}
