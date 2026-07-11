//! node:zlib — the `zlib.constants` object (flush values, return codes,
//! compression levels/strategies, window/mem bounds, the codec enum, and the
//! Brotli operation/param/mode constants). Real fixed values from the zlib and
//! Brotli headers — no fabrication.

use rts_engine::heap::shapes::alloc_shaped_object_owned;

/// `(name, value)` pairs for `zlib.constants`.
fn entries() -> Vec<(&'static str, f64)> {
    vec![
        // Flush values.
        ("Z_NO_FLUSH", 0.0),
        ("Z_PARTIAL_FLUSH", 1.0),
        ("Z_SYNC_FLUSH", 2.0),
        ("Z_FULL_FLUSH", 3.0),
        ("Z_FINISH", 4.0),
        ("Z_BLOCK", 5.0),
        ("Z_TREES", 6.0),
        // Return codes.
        ("Z_OK", 0.0),
        ("Z_STREAM_END", 1.0),
        ("Z_NEED_DICT", 2.0),
        ("Z_ERRNO", -1.0),
        ("Z_STREAM_ERROR", -2.0),
        ("Z_DATA_ERROR", -3.0),
        ("Z_MEM_ERROR", -4.0),
        ("Z_BUF_ERROR", -5.0),
        ("Z_VERSION_ERROR", -6.0),
        // Compression levels.
        ("Z_NO_COMPRESSION", 0.0),
        ("Z_BEST_SPEED", 1.0),
        ("Z_BEST_COMPRESSION", 9.0),
        ("Z_DEFAULT_COMPRESSION", -1.0),
        // Strategies.
        ("Z_FILTERED", 1.0),
        ("Z_HUFFMAN_ONLY", 2.0),
        ("Z_RLE", 3.0),
        ("Z_FIXED", 4.0),
        ("Z_DEFAULT_STRATEGY", 0.0),
        // Tuning bounds (Node's own).
        ("Z_DEFAULT_WINDOWBITS", 15.0),
        ("Z_MIN_WINDOWBITS", 8.0),
        ("Z_MAX_WINDOWBITS", 15.0),
        ("Z_DEFAULT_CHUNK", 16384.0),
        ("Z_MIN_CHUNK", 64.0),
        ("Z_MAX_CHUNK", f64::INFINITY),
        ("Z_DEFAULT_MEMLEVEL", 8.0),
        ("Z_MIN_MEMLEVEL", 1.0),
        ("Z_MAX_MEMLEVEL", 9.0),
        ("Z_MIN_LEVEL", -1.0),
        ("Z_MAX_LEVEL", 9.0),
        ("Z_DEFAULT_LEVEL", -1.0),
        // Codec enum.
        ("DEFLATE", 1.0),
        ("INFLATE", 2.0),
        ("GZIP", 3.0),
        ("GUNZIP", 4.0),
        ("DEFLATERAW", 5.0),
        ("INFLATERAW", 6.0),
        ("UNZIP", 7.0),
        ("BROTLI_DECODE", 8.0),
        ("BROTLI_ENCODE", 9.0),
        // Brotli operations.
        ("BROTLI_OPERATION_PROCESS", 0.0),
        ("BROTLI_OPERATION_FLUSH", 1.0),
        ("BROTLI_OPERATION_FINISH", 2.0),
        ("BROTLI_OPERATION_EMIT_METADATA", 3.0),
        // Brotli encoder params.
        ("BROTLI_PARAM_MODE", 0.0),
        ("BROTLI_MODE_GENERIC", 0.0),
        ("BROTLI_MODE_TEXT", 1.0),
        ("BROTLI_MODE_FONT", 2.0),
        ("BROTLI_PARAM_QUALITY", 1.0),
        ("BROTLI_PARAM_LGWIN", 2.0),
        ("BROTLI_PARAM_LGBLOCK", 3.0),
        ("BROTLI_PARAM_SIZE_HINT", 5.0),
        ("BROTLI_MIN_QUALITY", 0.0),
        ("BROTLI_MAX_QUALITY", 11.0),
        ("BROTLI_DEFAULT_QUALITY", 11.0),
        ("BROTLI_MIN_WINDOW_BITS", 10.0),
        ("BROTLI_MAX_WINDOW_BITS", 24.0),
        ("BROTLI_DEFAULT_WINDOW", 22.0),
        // Brotli decoder result codes.
        ("BROTLI_DECODER_RESULT_ERROR", 0.0),
        ("BROTLI_DECODER_RESULT_SUCCESS", 1.0),
        ("BROTLI_DECODER_RESULT_NEEDS_MORE_INPUT", 2.0),
        ("BROTLI_DECODER_RESULT_NEEDS_MORE_OUTPUT", 3.0),
    ]
}

/// `zlib.constants` — a `Constant` property getter returning the frozen object.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ZLIB_CONSTANTS() -> u64 {
    let es = entries();
    let keys: Vec<String> = es.iter().map(|(k, _)| k.to_string()).collect();
    let values: Vec<i64> = es.iter().map(|(_, v)| v.to_bits() as i64).collect();
    // Raw object handle — the engine reboxes it as an object (`: object` ts-sig).
    alloc_shaped_object_owned(keys, &values)
}
