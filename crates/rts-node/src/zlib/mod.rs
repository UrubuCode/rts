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
        ret_class: None,
        pure: false,
        emit: None,
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
        .member(s::deflate_sync_entry())
        .member(s::inflate_sync_entry())
        .member(s::deflate_raw_sync_entry())
        .member(s::inflate_raw_sync_entry())
        .member(s::gzip_sync_entry())
        .member(s::gunzip_sync_entry())
        .member(s::unzip_sync_entry())
        .member(s::brotli_compress_sync_entry())
        .member(s::brotli_decompress_sync_entry())
        .member(s::gzip_sync_level_entry())
        .member(s::deflate_sync_level_entry())
        .member(symbols::crc32_entry())
        .member(symbols::crc32_prev_entry())
        .member(make("constants", "__RTS_FN_NODE_ZLIB_CONSTANTS", sig!(=> Handle), "constants: object", constants::__RTS_FN_NODE_ZLIB_CONSTANTS as *const u8, MemberKind::Constant, MemberFlags::NONE))
        .done();
}
