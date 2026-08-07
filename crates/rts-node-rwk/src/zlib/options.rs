//! Reading a `ZlibOptions`/`BrotliOptions` argument, and reading an input
//! buffer.
//!
//! Every reader here takes `&mut Context` rather than reaching for the
//! ambient one. That is not a style choice: each is called from inside
//! `entry::with_runtime`, where the ambient form takes a SECOND `RefCell`
//! borrow and an `extern "C"` frame cannot unwind past the panic — the
//! process aborts. Giving the helper the context is the version that can only
//! be called correctly, which is the rule `rts-core-rwk`'s `is_array_in` doc
//! states and the reason that pair exists at all.

use rts_core_rwk::entry::{self, Context};

use super::codec::Settings;

/// `Z_SYNC_FLUSH`, the one flush constant this module acts on — see
/// [`Settings::tolerant`] and `mod.rs`'s refusal list for the rest.
const Z_SYNC_FLUSH: f64 = 2.0;
/// `BROTLI_PARAM_QUALITY`, as a property key on `options.params`.
const BROTLI_PARAM_QUALITY: &str = "1";
/// `BROTLI_PARAM_LGWIN`.
const BROTLI_PARAM_LGWIN: &str = "2";

/// A numeric member of an options object, `None` when the object is absent or
/// does not carry it.
///
/// `entry::number_of` and NOT `entry::text_of`/`to_boolean`: this is asking
/// WHAT a value is, and a coercion used as a type test is the defect class
/// that made `node:worker_threads` cross every number as a string. A
/// `{ level: "9" }` therefore reads as absent rather than as 9 — Node itself
/// rejects it with `ERR_INVALID_ARG_TYPE`, so falling back to the default is
/// the closer of the two answers available without a throw.
fn number_member(context: &mut Context, options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_in(context);
    if options == absent || !entry::is_object(context, options) {
        return None;
    }
    let value = entry::get_member(context, options, name);
    if value == absent {
        return None;
    }
    entry::number_of(value)
}

/// The options object resolved into what this module acts on.
pub(super) fn settings(context: &mut Context, options: u64) -> Settings {
    let mut settings = Settings::default();
    if let Some(level) = number_member(context, options, "level") {
        // `Z_DEFAULT_COMPRESSION` is -1 and every other negative is out of
        // range; both resolve to zlib's own default rather than to level 0,
        // which is "store, do not compress" and would be a silently
        // different answer.
        settings.level = match level < 0.0 {
            true => 6,
            false => level.clamp(0.0, 9.0) as u32,
        };
    }
    if let Some(size) = number_member(context, options, "chunkSize") {
        if size >= 64.0 {
            settings.chunk_size = size as usize;
        }
    }
    if let Some(cap) = number_member(context, options, "maxOutputLength") {
        if cap >= 0.0 {
            settings.max_output = Some(cap as usize);
        }
    }
    if let Some(flush) = number_member(context, options, "finishFlush") {
        settings.tolerant = flush == Z_SYNC_FLUSH;
    }
    brotli_params(context, options, &mut settings);
    settings
}

/// `BrotliOptions.params`, whose keys are the `BROTLI_PARAM_*` constants.
///
/// # The limit, named
///
/// The keys are read as the decimal NAMES `"1"`/`"2"`, through
/// `entry::get_member`, because that is the only property read on the host
/// surface that takes a context — `get_indexed` is ambient and calling it
/// from inside this borrow would abort the process. A `params` object whose
/// integer keys this engine stores as array elements rather than as named
/// properties therefore reads as absent, and the Brotli defaults (quality 11,
/// lgwin 22) apply. That is Node's own default rather than a fabricated
/// value, but it is a knob that can be set and not honoured, so it is stated
/// here rather than discovered.
fn brotli_params(context: &mut Context, options: u64, settings: &mut Settings) {
    let absent = entry::undefined_in(context);
    if options == absent || !entry::is_object(context, options) {
        return;
    }
    let params = entry::get_member(context, options, "params");
    if params == absent || !entry::is_object(context, params) {
        return;
    }
    if let Some(quality) = number_member(context, params, BROTLI_PARAM_QUALITY) {
        settings.quality = quality.clamp(0.0, 11.0) as u32;
    }
    if let Some(window) = number_member(context, params, BROTLI_PARAM_LGWIN) {
        settings.lgwin = window.clamp(10.0, 24.0) as u32;
    }
}

/// The input bytes of a `Buffer`/`TypedArray`/`DataView`/`ArrayBuffer`, or
/// the UTF-8 of a `string` — Node's `InputType`, whose string arm is fixed at
/// UTF-8 (there is no `inputEncoding` parameter for this module).
///
/// `None` for anything else, which is what makes `deflateSync(42)` answer
/// `undefined` rather than compressing the text `"42"`. `entry::string_in`
/// and NOT `entry::text_in`: the second is `ToString` and would turn the
/// number 42, `null`, and every object into input bytes — a wrong answer that
/// runs, where Node raises `ERR_INVALID_ARG_TYPE`.
pub(super) fn input_bytes(context: &Context, value: u64) -> Option<Vec<u8>> {
    if let Some(bytes) = entry::bytes_of(context, value) {
        return Some(bytes);
    }
    entry::string_in(context, value).map(String::into_bytes)
}

/// Node's `(buffer[, options], callback)` overload for a callback-shaped
/// convenience function, resolved.
///
/// `crate::fs::options_and_listener` is the one that already does this shift
/// and is `pub(crate)`; this calls it rather than being a second copy. It is
/// reached from OUTSIDE a borrow — it takes its own, so calling it from
/// inside one would abort.
pub(super) fn options_and_callback(options: u64, callback: u64) -> (u64, u64) {
    crate::fs::options_and_listener(options, callback)
}
