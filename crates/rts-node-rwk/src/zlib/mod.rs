//! `node:zlib` — DEFLATE/zlib, gzip and Brotli: the one-shot buffer
//! functions, their callback twins, `crc32`, the nine streaming
//! `stream.Transform` classes and their factories, and `constants`.
//!
//! # Reuse check (`reuse-check` skill, run before writing anything here)
//!
//! Searched, and what was found:
//!
//! - `rts-cranelift` — nothing. Compression is heap-and-bytes work, not a
//!   machine capability; no entry in the skill's "search the machine first"
//!   table (tags, shapes, layouts, ABI, GC, scheduler) is about a codec.
//! - `rts_core_rwk::entry::modules` — `bytes_of`/`make_buffer` are the byte
//!   in/out pair, `make_prototype`/`make_instance`/`set_prototype_in` are the
//!   class recipe, `module_at_name` is how one module reaches another's
//!   namespace. All used; none reimplemented. There is no throw, no `Error`
//!   constructor, and no `get_indexed` that takes a context — the three gaps
//!   this module's answers are shaped by.
//! - This crate — `crypto/hash.rs` already holds native codec state across
//!   calls in a `Mutex<HashMap<u64, _>>` keyed by a hidden number, and names
//!   `fs/fd.rs`/`fs/dir.rs` as doing the same. `streams.rs` follows that,
//!   which is why it is not a new table. `crate::fs::options_and_listener` is
//!   the `(x[, options], callback)` shift and is `pub(crate)`; `options.rs`
//!   calls it rather than writing a second one. `stream/duplex.rs` is the
//!   real `Transform`, reached through `node:stream`'s namespace.
//! - `crates/rts-node/src/zlib/` is a DIFFERENT crate — the old engine's, over
//!   `AbiType`/`StrPtr`/a symbol table this crate does not have. Nothing is
//!   taken from it. `docs/reference/node/zlib.md`'s §5.2 ABI table and §5.8
//!   phase plan describe that engine and do not apply here; what transfers is
//!   §5.1's crate choices (`flate2` with `rust_backend`, `brotli`), §3's
//!   defaults, §4's semantics and §6's test plan.
//!
//! # Buffers, not a second byte container
//!
//! Input is read with `entry::bytes_of` and results answer with
//! `entry::make_buffer` — the real `Buffer` class `rts-core-rwk` builds, so
//! `Buffer.isBuffer(zlib.gzipSync(x))` is `true`.
//!
//! # No throw, and what stands in for it
//!
//! Nothing on this crate's host surface throws (see `crypto/kdf.rs`'s doc).
//! So:
//!
//! - a `*Sync` function answers **`undefined`** on failure. NEVER an empty
//!   `Buffer`: an empty buffer is a legitimate result — the gzip of `""`
//!   decompresses to nothing — so the two must not share an answer. Three
//!   functions here (`brotliCompressSync`, `brotliDecompressSync`,
//!   `unzipSync`) previously ignored their codec's error and answered
//!   whatever their output vector held, which is exactly that collision.
//! - a callback function's error argument is a **`string`**, not an `Error`.
//!   `if (err)` works; `err instanceof Error`, `err.code` and `err.message`
//!   do not.
//! - a streaming class relays the same string through `_transform`'s
//!   callback, which `node:stream` turns into an `'error'` event.
//!
//! # Timing: the callback forms are synchronous
//!
//! `gzip(buf, cb)` runs the codec on the calling thread and invokes `cb`
//! before returning. Node defers it to the libuv threadpool. There is no
//! threadpool and no event loop reachable from this crate, and the choice was
//! between refusing the nine callback members outright and running them
//! early; `oneshot.rs`'s doc records why running them won. A program that
//! depends on the deferral observes a difference.
//!
//! # Not implemented, by name
//!
//! - **The whole Zstd sub-surface**: `ZstdCompress`, `ZstdDecompress`,
//!   `createZstdCompress`, `createZstdDecompress`, `zstdCompress`,
//!   `zstdCompressSync`, `zstdDecompress`, `zstdDecompressSync`, and every
//!   `ZSTD_*` constant. No zstd codec is a dependency of this crate, and
//!   adding one means editing `Cargo.toml`, which this module does not own.
//!   `docs/reference/node/zlib.md` §7 also leaves the pure-Rust-vs-C crate
//!   choice open, and Node marks the surface Experimental itself. The
//!   constants are absent rather than present-and-inert, because a constant
//!   that names a codec nothing implements reads as support.
//! - **`zlibBase.params(level, strategy, callback)`**. `flate2`'s `write::`
//!   adapters take their level at construction and expose no mid-stream
//!   change; reaching for the low-level `Compress` to get it would be the
//!   second codec path `codec.rs`'s doc refuses. Absent, so it reads
//!   `undefined`.
//! - **`ZlibBase` itself**, as a named export. Node does not export it
//!   either ("internal base; not directly exported/constructible", §2.1); the
//!   nine concrete classes are here and each carries `flush`/`reset`/`close`/
//!   `bytesWritten` directly.
//! - **`info: true`**, on every convenience function. Its `{ buffer, engine }`
//!   shape hands back the one-shot call's live engine instance, and the
//!   one-shot path here is a `Sink` that is finished and dropped inside the
//!   call — there is no instance to hand back. An `{ buffer, engine:
//!   undefined }` would be the plausible-looking half-answer this repository
//!   refuses.
//! - **`dictionary`**, on the Deflate/Inflate family. `flate2`'s `write::`
//!   adapters have no dictionary entry point.
//! - **`windowBits`, `memLevel`, `strategy`**. Same reason: not settable
//!   through the `write::` adapters this module's single codec path is built
//!   on. Passing them is a no-op rather than an error, which is stated here
//!   because it cannot be reported there.
//! - **`flush`, and `flush(kind)`'s `kind` argument.** The only flush
//!   constant acted on is `finishFlush: Z_SYNC_FLUSH`, which selects the
//!   documented "return partial data on a truncated stream" behaviour (§4).
//!   Every other `Z_*`/`BROTLI_OPERATION_*` flush value is accepted and
//!   ignored; `zlibBase.flush()` always performs the codec's own sync flush.
//! - **`BrotliOptions.params` keys other than `BROTLI_PARAM_QUALITY` and
//!   `BROTLI_PARAM_LGWIN`** — `BROTLI_PARAM_MODE`, `LGBLOCK`, `SIZE_HINT`,
//!   `LARGE_WINDOW`, `NPOSTFIX`, `NDIRECT`,
//!   `DISABLE_LITERAL_CONTEXT_MODELING`, and both
//!   `BROTLI_DECODER_PARAM_*` — none is reachable through
//!   `brotli::CompressorWriter`/`DecompressorWriter`. `options.rs` also names
//!   the one way even the two supported keys can go unread.
//! - **`zlib.constants` is not frozen.** Nothing here freezes an object; a
//!   program that assigns to `zlib.constants.Z_OK` changes it.
//! - **Deprecated top-level constant access** (`zlib.Z_NO_FLUSH` rather than
//!   `zlib.constants.Z_NO_FLUSH`). Node deprecates it; it is not new surface.

mod codec;
mod oneshot;
mod options;
mod streams;

use rts_core_rwk::entry::{self, Context};

/// The namespace `node:zlib` is.
pub fn namespace(context: &mut Context) -> u64 {
    let namespace = entry::make_namespace(context, oneshot::MEMBERS);
    streams::install(context, namespace);
    let constants = constants_table(context);
    entry::put_member(context, namespace, "constants", constants);
    namespace
}

/// `zlib.constants`.
///
/// Every non-Zstd constant `docs/reference/node/zlib.md` §2.3 lists, including
/// the ones nothing here reads back — a program passing
/// `{ strategy: zlib.constants.Z_RLE }` needs the value to exist even where
/// the option is refused above, and a missing constant would make it
/// `undefined` at the call site rather than at the option.
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
        ("Z_DEFAULT_WINDOWBITS", 15.0),
        ("Z_MIN_WINDOWBITS", 8.0),
        ("Z_MAX_WINDOWBITS", 15.0),
        ("Z_DEFAULT_MEMLEVEL", 8.0),
        ("Z_MIN_MEMLEVEL", 1.0),
        ("Z_MAX_MEMLEVEL", 9.0),
        ("Z_MIN_LEVEL", -1.0),
        ("Z_MAX_LEVEL", 9.0),
        ("Z_DEFAULT_LEVEL", -1.0),
        ("Z_MIN_CHUNK", 64.0),
        ("Z_DEFAULT_CHUNK", 16384.0),
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
        ("BROTLI_MIN_INPUT_BLOCK_BITS", 16.0),
        ("BROTLI_MAX_INPUT_BLOCK_BITS", 24.0),
        ("BROTLI_PARAM_DISABLE_LITERAL_CONTEXT_MODELING", 4.0),
        ("BROTLI_PARAM_SIZE_HINT", 5.0),
        ("BROTLI_PARAM_LARGE_WINDOW", 6.0),
        ("BROTLI_PARAM_NPOSTFIX", 7.0),
        ("BROTLI_PARAM_NDIRECT", 8.0),
        ("BROTLI_MAX_NPOSTFIX", 3.0),
        ("BROTLI_DECODER_PARAM_DISABLE_RING_BUFFER_REALLOCATION", 0.0),
        ("BROTLI_DECODER_PARAM_LARGE_WINDOW", 1.0),
        // Format constants — `zlib.constants.<FORMAT>`, the argument to
        // `zlib.createDeflate`/`createGzip`/etc.'s underlying dispatch and to
        // `ZLIB_VERNUM`. Node's `deps/zlib`/`lib/zlib.js` numbering.
        ("DEFLATE", 1.0),
        ("INFLATE", 2.0),
        ("GZIP", 3.0),
        ("GUNZIP", 4.0),
        ("DEFLATERAW", 5.0),
        ("INFLATERAW", 6.0),
        ("UNZIP", 7.0),
        ("BROTLI_DECODE", 8.0),
        ("BROTLI_ENCODE", 9.0),
    ];
    for (name, value) in entries {
        let held = entry::make_number(*value);
        entry::put_member(context, object, name, held);
    }
    object
}
