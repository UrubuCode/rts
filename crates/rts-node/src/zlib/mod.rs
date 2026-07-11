//! `node:zlib` — the synchronous compression surface: deflate/inflate,
//! deflateRaw/inflateRaw, gzip/gunzip, unzip (auto-detect), brotliCompress/
//! brotliDecompress, plus the `constants` object. Real pure-Rust codecs
//! (flate2/miniz_oxide + brotli), Buffer→Buffer. No stubs.
//!
//! Deferred (need the async event loop / the stream subsystem): the callback
//! forms (`gzip(buf, cb)` …), the transform-stream classes (`Gzip`/`Gunzip`/
//! `Deflate`/`BrotliCompress`/… + `createGzip` …), and the experimental Zstd
//! sub-surface. The full synchronous `*Sync` surface + constants is implemented.
//!
//! Module layout: `codec` (the compress/decompress cores), `constants`
//! (`zlib.constants`), `words` (byte read / Buffer), `symbols` (extern points).

mod codec;
mod constants;
mod symbols;
mod words;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn func(name: &str, symbol: &str, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig!(Handle => Handle), ts, fp, MemberKind::Function, MemberFlags::THROWS)
}

fn make(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8, kind: MemberKind, flags: MemberFlags) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `node:zlib` synchronous surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:zlib")
        .doc(
            "Synchronous compression (node:zlib): deflate/inflate(+Raw), gzip/ \
             gunzip, unzip, brotliCompress/brotliDecompress (all *Sync), and \
             constants. Pure-Rust codecs.",
        )
        .member(func("deflateSync", "__RTS_FN_NODE_ZLIB_DEFLATE_SYNC", "deflateSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_DEFLATE_SYNC as *const u8))
        .member(func("inflateSync", "__RTS_FN_NODE_ZLIB_INFLATE_SYNC", "inflateSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_INFLATE_SYNC as *const u8))
        .member(func("deflateRawSync", "__RTS_FN_NODE_ZLIB_DEFLATE_RAW_SYNC", "deflateRawSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_DEFLATE_RAW_SYNC as *const u8))
        .member(func("inflateRawSync", "__RTS_FN_NODE_ZLIB_INFLATE_RAW_SYNC", "inflateRawSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_INFLATE_RAW_SYNC as *const u8))
        .member(func("gzipSync", "__RTS_FN_NODE_ZLIB_GZIP_SYNC", "gzipSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_GZIP_SYNC as *const u8))
        .member(func("gunzipSync", "__RTS_FN_NODE_ZLIB_GUNZIP_SYNC", "gunzipSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_GUNZIP_SYNC as *const u8))
        .member(func("unzipSync", "__RTS_FN_NODE_ZLIB_UNZIP_SYNC", "unzipSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_UNZIP_SYNC as *const u8))
        .member(func("brotliCompressSync", "__RTS_FN_NODE_ZLIB_BROTLI_COMPRESS_SYNC", "brotliCompressSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_BROTLI_COMPRESS_SYNC as *const u8))
        .member(func("brotliDecompressSync", "__RTS_FN_NODE_ZLIB_BROTLI_DECOMPRESS_SYNC", "brotliDecompressSync(buffer: object): object", s::__RTS_FN_NODE_ZLIB_BROTLI_DECOMPRESS_SYNC as *const u8))
        .member(make("constants", "__RTS_FN_NODE_ZLIB_CONSTANTS", sig!(=> Handle), "constants: object", constants::__RTS_FN_NODE_ZLIB_CONSTANTS as *const u8, MemberKind::Constant, MemberFlags::NONE))
        .done();
}
