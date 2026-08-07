//! The convenience functions: `*Sync`, their callback twins, and `crc32`.
//!
//! # Why the callback forms exist here and are still synchronous
//!
//! `docs/reference/node/zlib.md` §5.3 puts every non-`Sync` form on a
//! `spawn_blocking` threadpool. There is no threadpool and no event loop
//! reachable from this crate (the gap `crypto/random.rs`'s doc names), so the
//! choice was between refusing nine members and running the same codec on the
//! calling thread before invoking the callback. Refusing loses `gzip(buf, cb)`
//! — the form most programs write — for a property (which turn of the loop
//! the callback runs on) that a compression call's own correctness does not
//! depend on. So they run, and the divergence is stated in `mod.rs`: the
//! callback fires BEFORE the call returns, not on a later tick. A program
//! that relies on the deferral observes a difference; a program that just
//! wants its bytes does not.
//!
//! # What the error argument is
//!
//! A `string`, not an `Error`. Nothing on the host surface constructs an
//! `Error` instance and this crate cannot throw, so the choices were `null`
//! (which reads as success — the `process.exit` defect exactly) or a truthy
//! value carrying the reason. `if (err)` works; `err instanceof Error` and
//! `err.code` do not, and that is named here rather than found at run time.

use rts_core_rwk::entry::{self, Provided};

use super::codec::{self, Kind};
use super::options;

/// One buffer through one codec, `undefined` on failure.
///
/// `undefined` and NOT an empty `Buffer` — an empty buffer is a legitimate
/// result (`gunzipSync` of the gzip of `""` IS empty), so answering it for a
/// failure makes the two indistinguishable. This module shipped that bug
/// once, in `brotliDecompressSync` and `unzipSync`, which ignored their
/// codec's error and answered whatever the output vector happened to hold.
fn sync_call(kind: Kind, buffer: u64, options: u64) -> u64 {
    entry::with_runtime(|context| {
        let Some(input) = options::input_bytes(context, buffer) else {
            return entry::undefined_in(context);
        };
        let settings = options::settings(context, options);
        match codec::one_shot(kind, &input, &settings) {
            Some(bytes) => entry::make_buffer(context, &bytes),
            None => entry::undefined_in(context),
        }
    })
}

/// The same, then `callback(error, result)`.
fn callback_call(kind: Kind, buffer: u64, options: u64, callback: u64) -> u64 {
    // OUTSIDE any borrow: this takes its own, and every `entry::call` below is
    // an ambient entry point that would abort inside one.
    let (options, callback) = options::options_and_callback(options, callback);
    let result = sync_call(kind, buffer, options);
    let absent = entry::undefined_value();
    if callback == absent {
        return absent;
    }
    if result == absent {
        let reason = entry::with_runtime(|context| {
            entry::make_string(context, "zlib: input could not be processed")
        });
        entry::call(callback, absent, reason, absent, absent, absent);
        return absent;
    }
    let nothing = entry::null_value();
    entry::call(callback, absent, nothing, result, absent, absent);
    absent
}

macro_rules! convenience {
    ($sync:ident, $async:ident, $kind:expr) => {
        extern "C" fn $sync(_e: u64, _this: u64, buffer: u64, options: u64, _c: u64, _d: u64) -> u64 {
            sync_call($kind, buffer, options)
        }
        extern "C" fn $async(_e: u64, _this: u64, buffer: u64, options: u64, callback: u64, _d: u64) -> u64 {
            callback_call($kind, buffer, options, callback)
        }
    };
}

convenience!(gzip_sync, gzip, Kind::Gzip);
convenience!(gunzip_sync, gunzip, Kind::Gunzip);
convenience!(deflate_sync, deflate, Kind::Deflate);
convenience!(inflate_sync, inflate, Kind::Inflate);
convenience!(deflate_raw_sync, deflate_raw, Kind::DeflateRaw);
convenience!(inflate_raw_sync, inflate_raw, Kind::InflateRaw);
convenience!(unzip_sync, unzip, Kind::Unzip);
convenience!(brotli_compress_sync, brotli_compress, Kind::BrotliCompress);
convenience!(brotli_decompress_sync, brotli_decompress, Kind::BrotliDecompress);

/// `zlib.crc32(data, value?)`.
///
/// `undefined` for a `data` that is not a string or a byte view, and for a
/// `value` that is present but not a number or outside the unsigned 32-bit
/// range — the two cases Node raises `ERR_INVALID_ARG_TYPE`/`ERR_OUT_OF_RANGE`
/// for. Coercing them instead would answer a checksum of something the caller
/// never passed.
extern "C" fn crc32(_e: u64, _this: u64, data: u64, value: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let Some(bytes) = options::input_bytes(context, data) else {
            return absent;
        };
        let seed = match value == absent {
            true => 0.0,
            false => match entry::number_of(value) {
                Some(number) => number,
                None => return absent,
            },
        };
        if !(0.0..=4_294_967_295.0).contains(&seed) || seed.fract() != 0.0 {
            return absent;
        }
        entry::make_number(f64::from(codec::crc32(&bytes, seed as u32)))
    })
}

/// Every member this file provides, for the namespace.
pub(super) const MEMBERS: &[(&str, Provided)] = &[
    ("gzipSync", gzip_sync),
    ("gzip", gzip),
    ("gunzipSync", gunzip_sync),
    ("gunzip", gunzip),
    ("deflateSync", deflate_sync),
    ("deflate", deflate),
    ("inflateSync", inflate_sync),
    ("inflate", inflate),
    ("deflateRawSync", deflate_raw_sync),
    ("deflateRaw", deflate_raw),
    ("inflateRawSync", inflate_raw_sync),
    ("inflateRaw", inflate_raw),
    ("unzipSync", unzip_sync),
    ("unzip", unzip),
    ("brotliCompressSync", brotli_compress_sync),
    ("brotliCompress", brotli_compress),
    ("brotliDecompressSync", brotli_decompress_sync),
    ("brotliDecompress", brotli_decompress),
    ("crc32", crc32),
];
