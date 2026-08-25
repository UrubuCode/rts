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
//!
//! # A BAD ARGUMENT throws, on both forms
//!
//! The paragraph above is about a codec that failed on real bytes. An argument
//! that is not bytes at all is a different answer, and it is Node's: a
//! synchronous throw, from `gunzip(1, cb)` exactly as from `gunzipSync(1)`,
//! before the callback is ever reached (`test-zlib-invalid-input.js` asserts
//! the async form throws rather than reporting). So the check runs once, in
//! [`prepare`], and both forms raise the same refusal.

use rts_core::entry::{self, Provided};

use super::codec::{self, Kind, Settings};
use super::options::{self, Refusal};

/// The input and the settings of one convenience call, or the first thing
/// wrong with them.
///
/// One function for both forms because they accept exactly the same arguments;
/// two would be two places for the answer to "what does `gzip` take" to drift,
/// and the sync/async split is about WHEN the result is delivered.
fn prepare(kind: Kind, buffer: u64, options: u64) -> Result<(Vec<u8>, Settings), Refusal> {
    entry::with_runtime(|context| {
        let Some(input) = options::input_bytes(context, buffer) else {
            return Err(Refusal::Input("buffer", buffer));
        };
        let settings = options::settings(context, kind, options)?;
        Ok((input, settings))
    })
}

/// One buffer through one codec, `undefined` on a codec failure.
///
/// `undefined` and NOT an empty `Buffer` — an empty buffer is a legitimate
/// result (`gunzipSync` of the gzip of `""` IS empty), so answering it for a
/// failure makes the two indistinguishable. This module shipped that bug
/// once, in `brotliDecompressSync` and `unzipSync`, which ignored their
/// codec's error and answered whatever the output vector happened to hold.
fn sync_call(kind: Kind, buffer: u64, options: u64) -> u64 {
    let prepared = prepare(kind, buffer, options);
    let (input, settings) = match prepared {
        Ok(pair) => pair,
        // Raised HERE and not inside `prepare`: raising builds an `Error` and
        // throws it, which takes its own borrow, and doing that from inside
        // the borrow that found the mistake aborts the process.
        Err(refusal) => {
            refusal.raise();
            return entry::undefined_value();
        }
    };
    match codec::one_shot(kind, &input, &settings) {
        Some(bytes) => entry::with_runtime(|context| entry::make_buffer(context, &bytes)),
        None => entry::undefined_value(),
    }
}

/// The same, then `callback(error, result)`.
fn callback_call(kind: Kind, buffer: u64, options: u64, callback: u64) -> u64 {
    // OUTSIDE any borrow: this takes its own, and every `entry::call` below is
    // an ambient entry point that would abort inside one.
    let (options, callback) = options::options_and_callback(options, callback);
    let absent = entry::undefined_value();
    // Checked before the codec runs and before the callback exists as far as
    // this call is concerned: a refused argument THROWS, and invoking the
    // callback afterwards would run it with a throw already pending.
    let (input, settings) = match prepare(kind, buffer, options) {
        Ok(pair) => pair,
        Err(refusal) => {
            refusal.raise();
            return absent;
        }
    };
    let result = match codec::one_shot(kind, &input, &settings) {
        Some(bytes) => entry::with_runtime(|context| entry::make_buffer(context, &bytes)),
        None => absent,
    };
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
/// Throws for a `data` that is not a string or a byte view, and for a `value`
/// that is present but not a number or outside the unsigned 32-bit range —
/// Node's `ERR_INVALID_ARG_TYPE` and `ERR_OUT_OF_RANGE`, which
/// `test-zlib-crc32.js` asserts by code for six kinds of bad `data` and four
/// of bad `value`. Coercing instead would answer a checksum of something the
/// caller never passed, and answering `undefined` — what this did — is a
/// number-shaped hole a program only finds later.
extern "C" fn crc32(_e: u64, _this: u64, data: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let prepared = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let Some(bytes) = options::input_bytes(context, data) else {
            return Err(Refusal::Input("data", data));
        };
        if value == absent {
            return Ok((bytes, 0));
        }
        let Some(seed) = entry::number_of(value) else {
            return Err(Refusal::OptionType("value", value));
        };
        // The fractional test is part of the RANGE and not a separate refusal:
        // Node's `validateUint32` reports `2.5` the same way it reports `-1`,
        // because a CRC seed is 32 bits and half of one is not a smaller seed.
        if !(0.0..=4_294_967_295.0).contains(&seed) || seed.fract() != 0.0 {
            return Err(Refusal::OptionRange("value", ">= 0 and <= 4294967295", value));
        }
        Ok((bytes, seed as u32))
    });
    match prepared {
        Ok((bytes, seed)) => entry::make_number(f64::from(codec::crc32(&bytes, seed))),
        // Outside the borrow above, for the reason `sync_call` states.
        Err(refusal) => {
            refusal.raise();
            entry::undefined_value()
        }
    }
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
