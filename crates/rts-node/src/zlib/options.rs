//! Reading a `ZlibOptions`/`BrotliOptions` argument, reading an input buffer —
//! and REFUSING either when Node refuses it.
//!
//! Every reader here takes `&mut Context` rather than reaching for the
//! ambient one. That is not a style choice: each is called from inside
//! `entry::with_runtime`, where the ambient form takes a SECOND `RefCell`
//! borrow and an `extern "C"` frame cannot unwind past the panic — the
//! process aborts. Giving the helper the context is the version that can only
//! be called correctly, which is the rule `rts-core`'s `is_array_in` doc
//! states and the reason that pair exists at all.
//!
//! # Why a refusal is RETURNED and not raised
//!
//! `crate::errors` is ambient — it builds an `Error` and throws it, taking its
//! own borrow — so raising from where the mistake is FOUND is the nested
//! borrow above, which aborts the process rather than failing a call. So every
//! check here answers a [`Refusal`] and the caller raises it after its borrow
//! has closed. The alternative, checking a second time outside the borrow, is
//! two places that must agree about what `zlib` accepts.
//!
//! # What Node refuses that this does not
//!
//! - **A `params` key that is not an integer** (`ERR_BROTLI_INVALID_PARAM`).
//!   Only the VALUES are checked here; a `{ params: { nope: 1 } }` is ignored
//!   rather than refused.
//! - **A bare `ArrayBuffer` as input.** Node accepts one and so does the
//!   message [`INPUT_TYPES`] prints, but `entry::bytes_of` answers only for a
//!   VIEW (`Buffer`/`TypedArray`/`DataView`), so one arrives here
//!   indistinguishable from a plain object and is refused. Stated rather than
//!   discovered: the fix is a runtime accessor, not a second byte reader here.

use rts_core::entry::{self, Context};

use super::codec::{Kind, Settings};

/// `Z_SYNC_FLUSH`, the one flush constant this module acts on — see
/// [`Settings::tolerant`] and `mod.rs`'s refusal list for the rest.
const Z_SYNC_FLUSH: f64 = 2.0;
/// `BROTLI_PARAM_QUALITY`, as a property key on `options.params`.
const BROTLI_PARAM_QUALITY: &str = "1";
/// `BROTLI_PARAM_LGWIN`.
const BROTLI_PARAM_LGWIN: &str = "2";

/// What Node's `ERR_INVALID_ARG_TYPE` says a zlib input may be, word for word
/// — `test-zlib-not-string-or-buffer.js` compares the whole sentence.
pub(super) const INPUT_TYPES: &str =
    "string or an instance of Buffer, TypedArray, DataView, or ArrayBuffer";

/// A refusal found inside a borrow, carried out of it to be raised.
///
/// It holds the offending VALUE and not a rendering of it, because the
/// rendering (`Received type number (23)`) is `crate::errors`' business and
/// stating it here would be the second spelling of a message this repository
/// keeps in one place.
pub(super) enum Refusal {
    /// The first argument is not one of [`INPUT_TYPES`]; the name is the one
    /// the message quotes (`"buffer"`, or `"data"` for `crc32`).
    Input(&'static str, u64),
    /// A numeric option that is not a number at all.
    OptionType(&'static str, u64),
    /// A numeric option outside the range Node documents for it.
    OptionRange(&'static str, &'static str, u64),
    /// A `params` entry whose value is neither a number nor a boolean.
    ParamValue(u64),
}

impl Refusal {
    /// Raises it. **Outside every runtime borrow** — see the module doc.
    pub(super) fn raise(self) {
        match self {
            Refusal::Input(name, value) => crate::errors::invalid_arg_type(name, INPUT_TYPES, value),
            Refusal::OptionType(name, value) => crate::errors::invalid_arg_type(name, "number", value),
            Refusal::OptionRange(name, range, value) => crate::errors::out_of_range(name, range, value),
            Refusal::ParamValue(value) => {
                crate::errors::invalid_arg_type("options.params[key]", "number", value)
            }
        }
    }
}

/// A numeric option, checked the way Node's own `checkFiniteNumber` checks it.
///
/// Four outcomes and each is Node's:
///
/// - absent → `Ok(None)`, the caller's default applies;
/// - **`NaN` → `Ok(None)` as well**, which reads as a bug until you see
///   `test-zlib-failed-init.js` assert that `createGzip({ level: NaN })` gives
///   a stream at `Z_DEFAULT_COMPRESSION` rather than a throw;
/// - not a number → `ERR_INVALID_ARG_TYPE`;
/// - infinite → `ERR_OUT_OF_RANGE`, whose expectation is the words *"a finite
///   number"* rather than a numeric range.
///
/// `entry::number_of` and NOT `entry::text_of`/`to_boolean`: this is asking
/// WHAT a value is, and a coercion used as a type test is the defect class
/// that made `node:worker_threads` cross every number as a string.
fn number_member(
    context: &mut Context,
    options: u64,
    name: &'static str,
    key: &str,
) -> Result<Option<f64>, Refusal> {
    let absent = entry::undefined_in(context);
    if options == absent || !entry::is_object(context, options) {
        return Ok(None);
    }
    let value = entry::get_member(context, options, key);
    if value == absent {
        return Ok(None);
    }
    let Some(number) = entry::number_of(value) else {
        return Err(Refusal::OptionType(name, value));
    };
    if number.is_nan() {
        return Ok(None);
    }
    if !number.is_finite() {
        return Err(Refusal::OptionRange(name, "a finite number", value));
    }
    Ok(Some(number))
}

/// The same, with the range Node documents applied to it.
///
/// `upper` is `f64::INFINITY` for the one-sided options, and `range` is the
/// message text rather than something derived from the bounds — Node writes
/// `">= 64"` for one side and `">= 9 and <= 15"` for two, and deriving the
/// second from the first is a formatting rule nobody asked for.
fn ranged_member(
    context: &mut Context,
    options: u64,
    name: &'static str,
    key: &str,
    range: &'static str,
    bounds: (f64, f64),
) -> Result<Option<f64>, Refusal> {
    let Some(number) = number_member(context, options, name, key)? else {
        return Ok(None);
    };
    if number < bounds.0 || number > bounds.1 {
        let value = entry::get_member(context, options, key);
        return Err(Refusal::OptionRange(name, range, value));
    }
    Ok(Some(number))
}

/// The options object resolved into what this module acts on, or the first
/// thing wrong with it.
///
/// # Why options this module IGNORES are still checked
///
/// `windowBits`, `memLevel` and `strategy` are on `mod.rs`'s refusal list —
/// nothing here can act on them, because the `flate2::write::` adapters expose
/// no setter. That is a reason to ignore a LEGAL value, not a reason to accept
/// an illegal one: `createGzip({ windowBits: 0 })` is a program that will
/// behave differently here than in Node no matter what, and the honest answer
/// is the one Node gives (`ERR_OUT_OF_RANGE`) rather than a stream that
/// silently used 15.
pub(super) fn settings(
    context: &mut Context,
    kind: Kind,
    options: u64,
) -> Result<Settings, Refusal> {
    let mut settings = Settings::default();
    if let Some(size) = ranged_member(
        context,
        options,
        "options.chunkSize",
        "chunkSize",
        ">= 64",
        (64.0, f64::INFINITY),
    )? {
        settings.chunk_size = size as usize;
    }
    if let Some(cap) = ranged_member(
        context,
        options,
        "options.maxOutputLength",
        "maxOutputLength",
        ">= 0",
        (0.0, f64::INFINITY),
    )? {
        settings.max_output = Some(cap as usize);
    }
    for (name, key) in [("options.flush", "flush"), ("options.finishFlush", "finishFlush")] {
        ranged_member(context, options, name, key, ">= 0 and <= 5", (0.0, 5.0))?;
    }
    if let Some(flush) = number_member(context, options, "options.finishFlush", "finishFlush")? {
        settings.tolerant = flush == Z_SYNC_FLUSH;
    }
    match kind.is_brotli() {
        true => brotli_params(context, options, &mut settings)?,
        false => zlib_knobs(context, kind, options, &mut settings)?,
    }
    Ok(settings)
}

/// `level`, `windowBits`, `memLevel` and `strategy` — the four a Brotli call
/// does not have, which is why they are asked for only when the codec is a
/// zlib one. Applying them to `brotliCompressSync(x, { level: 20 })` would
/// refuse a call Node accepts and ignores.
fn zlib_knobs(
    context: &mut Context,
    kind: Kind,
    options: u64,
    settings: &mut Settings,
) -> Result<(), Refusal> {
    if let Some(level) = ranged_member(
        context,
        options,
        "options.level",
        "level",
        ">= -1 and <= 9",
        (-1.0, 9.0),
    )? {
        // `Z_DEFAULT_COMPRESSION` is -1, and it resolves to zlib's own default
        // rather than to level 0 — which is "store, do not compress" and would
        // be a silently different answer.
        settings.level = match level < 0.0 {
            true => 6,
            false => level as u32,
        };
    }
    // `windowBits: 0` means "read it from the stream header" and is legal for
    // a decompressor ONLY; `test-zlib-zero-windowBits.js` asserts both halves,
    // which is why this is the one option whose legal set depends on the kind.
    let zero_allowed = kind.decompresses()
        && number_member(context, options, "options.windowBits", "windowBits")? == Some(0.0);
    if !zero_allowed {
        ranged_member(
            context,
            options,
            "options.windowBits",
            "windowBits",
            ">= 9 and <= 15",
            (9.0, 15.0),
        )?;
    }
    ranged_member(context, options, "options.memLevel", "memLevel", ">= 1 and <= 9", (1.0, 9.0))?;
    ranged_member(context, options, "options.strategy", "strategy", ">= 0 and <= 4", (0.0, 4.0))?;
    Ok(())
}

/// `BrotliOptions.params`, whose keys are the `BROTLI_PARAM_*` constants.
///
/// Every value is checked — a `params` entry must be a number or a boolean,
/// which is what `test-zlib-invalid-arg-value-brotli-compress.js` asserts for
/// `{ [BROTLI_PARAM_MODE]: 'lol' }` — while only two are READ, for the reason
/// below.
///
/// # The limit, named
///
/// The two read keys are read as the decimal NAMES `"1"`/`"2"`, through
/// `entry::get_member`, because that is the only property read on the host
/// surface that takes a context — `get_indexed` is ambient and calling it
/// from inside this borrow would abort the process. A `params` object whose
/// integer keys this engine stores as array elements rather than as named
/// properties therefore reads as absent, and the Brotli defaults (quality 11,
/// lgwin 22) apply. That is Node's own default rather than a fabricated
/// value, but it is a knob that can be set and not honoured, so it is stated
/// here rather than discovered.
fn brotli_params(
    context: &mut Context,
    options: u64,
    settings: &mut Settings,
) -> Result<(), Refusal> {
    let absent = entry::undefined_in(context);
    if options == absent || !entry::is_object(context, options) {
        return Ok(());
    }
    let params = entry::get_member(context, options, "params");
    if params == absent || !entry::is_object(context, params) {
        return Ok(());
    }
    let truth = entry::boolean_value(true);
    let untruth = entry::boolean_value(false);
    for key in entry::member_names(context, params) {
        let value = entry::get_member(context, params, &key);
        let acceptable =
            entry::number_of(value).is_some() || value == truth || value == untruth;
        if !acceptable {
            return Err(Refusal::ParamValue(value));
        }
    }
    if let Some(quality) = number_member(context, params, "options.params", BROTLI_PARAM_QUALITY)? {
        settings.quality = quality.clamp(0.0, 11.0) as u32;
    }
    if let Some(window) = number_member(context, params, "options.params", BROTLI_PARAM_LGWIN)? {
        settings.lgwin = window.clamp(10.0, 24.0) as u32;
    }
    Ok(())
}

/// The input bytes of a `Buffer`/`TypedArray`/`DataView`, or the UTF-8 of a
/// `string` — Node's `InputType`, whose string arm is fixed at UTF-8 (there is
/// no `inputEncoding` parameter for this module).
///
/// `None` for anything else, which is what makes `deflateSync(42)` REFUSE
/// rather than compress the text `"42"`. `entry::string_in` and NOT
/// `entry::text_in`: the second is `ToString` and would turn the number 42,
/// `null`, and every object into input bytes — a wrong answer that runs, where
/// Node raises `ERR_INVALID_ARG_TYPE`.
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
