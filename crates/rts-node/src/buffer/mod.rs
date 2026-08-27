//! `node:buffer` — what is left once `Buffer` itself is a real class.
//!
//! # What reuse-check found
//!
//! Everything byte-shaped here already has ONE implementation and this module
//! calls it rather than growing a second:
//!
//! - The `Buffer` class — constructor, statics, every instance method and the
//!   six codecs — lives in `rts-core::entry::buffer`, reached through
//!   [`entry::buffer_class`]. `layering.md` §6 puts it there ("`Buffer` =
//!   `Uint8Array` subclass + codecs in PR; engine surfaces the global"), so a
//!   thin module here is not a shortcut — it is the placement. **Nothing in
//!   this folder redefines the class**, and `lib.rs` binds the `Buffer` global
//!   to the same cell for the same reason.
//! - base64 — [`entry::encode_base64`]/[`entry::decode_base64`], the functions
//!   `Buffer`'s own `'base64'` encoding uses. Two decoders answering one name
//!   is what moving `Buffer` out of this file was for.
//! - the transcode codecs — [`entry::encode_text`]/[`entry::decode_bytes`]/
//!   [`entry::canonical_encoding`], likewise.
//! - promises — [`entry::settled`], the same already-settled promise
//!   `fs.promises` answers with; `Blob`'s three async methods have no real
//!   asynchrony to express (the bytes are resident), which is exactly the case
//!   that helper documents.
//! - Blob byte storage — nothing answers it. The nearest is
//!   `rts-core`'s buffer table, which differs because it backs a MUTABLE
//!   `ArrayBuffer` a program can write through; a `Blob` is immutable after
//!   construction and its slices share bytes, so [`blob`]'s table holds an
//!   `Arc<[u8]>` and hands out `(offset, length)` views over it. The
//!   instance↔table keying follows `string_decoder`/`fs::dir` exactly rather
//!   than inventing a third shape.
//!
//! # What this module owns
//!
//! The module-level surface Node's `buffer` module has BESIDE the class:
//! `atob`/`btoa`, `isAscii`/`isUtf8`, `transcode`, `resolveObjectURL`,
//! `constants`/`kMaxLength`/`kStringMaxLength`/`INSPECT_MAX_BYTES`, and the
//! `Blob`/`File` classes ([`blob`]).
//!
//! # Not implemented, by name
//!
//! - **`blob.stream()`** — a Web `ReadableStream`. Nothing in this engine is
//!   one, and the honest shape of the gap is an absent member rather than an
//!   object that reads as a stream and delivers nothing. Reference §7 defers
//!   it for the same reason.
//! - **`Blob`/`File` sources that are a raw `ArrayBuffer`** (as opposed to a
//!   view over one: a `Buffer`, a typed array or a `DataView`, all of which
//!   work). [`entry::bytes_of`] reads bytes through a view, and an
//!   `ArrayBuffer` has none — so such a part contributes **zero bytes** and is
//!   named here rather than silently changing `blob.size`. The missing host
//!   capability is "bytes of an `ArrayBuffer` cell"; `isAscii`/`isUtf8` raise
//!   `ERR_INVALID_ARG_TYPE` for inputs without a readable view.
//! - **`resolveObjectURL(id)`** answers `undefined` for every `id`, and that is
//!   a correct total answer rather than a stub: the ids it resolves are minted
//!   only by `URL.createObjectURL`, which `node:url` refuses by name for want
//!   of a shared registry (reference §5.7/§7). Nothing can mint one, so
//!   nothing can be found.
//! - **`atob`/`btoa` as ambient globals.** They are members of this namespace
//!   and reachable by import; the global spelling of both is `rts-std`'s
//!   (`globals/text.rs`), over the same codec. `Blob` and `File` ARE globals,
//!   bound in `lib.rs` to the very cells [`blob::classes`] mints — see that
//!   function for why one class has to answer both spellings.
//! - **Every `ERR_*` throw** in the reference's error column. A native entry
//!   point here cannot raise a catchable JS exception — the limit `events` and
//!   `string_decoder` already state — so `atob` on invalid base64, `btoa` above
//!   `U+00FF`, and `transcode` with an unsupported encoding each take the
//!   no-throw stand-in documented on the function itself.
//! - **`blob.size`/`blob.type`/`file.name`/`file.lastModified` are own data
//!   properties, not read-only accessors**, so a program can assign to them.
//!   The same trade a typed array's `length` already makes in `rts-core`.

pub(crate) mod blob;

use rts_core::entry::{self, Context, Provided};
use std::sync::atomic::{AtomicU64, Ordering};

/// The namespace `node:buffer` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("transcode", transcode),
        ("isAscii", is_ascii),
        ("isUtf8", is_utf8),
        ("SlowBuffer", slow_buffer_native),
        ("resolveObjectURL", resolve_object_url),
    ];
    let namespace = entry::make_namespace(context, members);
    let global = entry::global_object(context);
    let atob_global = entry::get_member(context, global, "atob");
    let btoa_global = entry::get_member(context, global, "btoa");
    entry::put_member(context, namespace, "atob", atob_global);
    entry::put_member(context, namespace, "btoa", btoa_global);

    let buffer = entry::buffer_class(context);
    entry::put_member(context, namespace, "Buffer", buffer);
    blob::install(context, namespace);

    let constants = entry::make_object(context);
    let max_length = entry::make_number(MAX_LENGTH);
    entry::put_member(context, constants, "MAX_LENGTH", max_length);
    let max_string_length = entry::make_number(MAX_STRING_LENGTH);
    entry::put_member(context, constants, "MAX_STRING_LENGTH", max_string_length);
    entry::put_member(context, namespace, "constants", constants);

    let k_max_length = entry::make_number(MAX_LENGTH);
    entry::put_member(context, namespace, "kMaxLength", k_max_length);
    let k_string_max_length = entry::make_number(MAX_STRING_LENGTH);
    entry::put_member(context, namespace, "kStringMaxLength", k_string_max_length);
    // Mutable in Node, and represented here by an accessor so assignments can
    // validate the non-negative numeric limit before `util.inspect` reads it.
    entry::define_accessor_in(
        context,
        namespace,
        "INSPECT_MAX_BYTES",
        inspect_max_bytes_get,
        Some(inspect_max_bytes_set),
    );

    namespace
}

/// This engine picks its own ceiling rather than copying a V8 build's — see
/// reference §7. `i32::MAX` is the widest byte count a proven-integer index
/// can represent exactly.
///
/// Read from `rts-core` rather than written again here, and that is not tidying:
/// this constant is what a program is TOLD (`buffer.constants.MAX_LENGTH`) and
/// `Buffer`'s own `validate::size` is what ENFORCES it. Two spellings of one
/// number means `Buffer.alloc(buffer.kMaxLength)` can be refused by the very
/// engine that published it — which is exactly what
/// `test-buffer-over-max-length.js` computes its argument from.
const MAX_LENGTH: f64 = entry::BUFFER_MAX_LENGTH;

/// The string ceiling is narrower than the byte ceiling because String cells
/// are materialised as UTF-16 code units. It is shared with `String.repeat`
/// through the core entry surface rather than duplicated here.
const MAX_STRING_LENGTH: f64 = entry::BUFFER_MAX_STRING_LENGTH;

static INSPECT_MAX_BYTES: AtomicU64 = AtomicU64::new(50.0f64.to_bits());

/// `buffer.transcode(source, fromEnc, toEnc)`. Decodes to a `String`
/// intermediate with `fromEnc`'s codec, then re-encodes with `toEnc`'s —
/// restricted to Node's own six binary-text encodings (no base64/hex); an
/// unsupported name answers a copy of `source` unchanged rather than throwing.
///
/// Answers a `Buffer`, which is what Node answers. It used to answer a
/// `Uint8Array` because that was the only shape a host could build; the
/// difference is observable (`Buffer.isBuffer`, every instance method), so
/// [`entry::make_buffer`] landing made this a wrong answer to correct rather
/// than a preference.
extern "C" fn transcode(_e: u64, _this: u64, source: u64, from_enc: u64, to_enc: u64, _d: u64) -> u64 {
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, source)).unwrap_or_default();
    // `string_in`, not `text_of`: an encoding argument is being asked WHAT it
    // is, and a coercion answers `"42"` for a number — which would then miss
    // the encoding table and look like an unsupported name rather than a wrong
    // type. Both reads happen outside any borrow.
    let names = entry::with_runtime(|context| {
        (entry::string_in(context, from_enc), entry::string_in(context, to_enc))
    });
    let (Some(from_name), Some(to_name)) = names else {
        return entry::with_runtime(|context| entry::make_buffer(context, &bytes));
    };
    let (Some(from_canon), Some(to_canon)) =
        (transcode_encoding(&from_name), transcode_encoding(&to_name))
    else {
        return entry::with_runtime(|context| entry::make_buffer(context, &bytes));
    };
    let text = entry::decode_bytes(&bytes, from_canon);
    let out = entry::encode_text(&text, to_canon).unwrap_or(bytes);
    entry::with_runtime(|context| entry::make_buffer(context, &out))
}

/// A [`transcode`] encoding name, restricted to Node's six there (no
/// base64/hex — see the reference's `TranscodeEncoding`).
fn transcode_encoding(name: &str) -> Option<&'static str> {
    match entry::canonical_encoding(name)? {
        "base64" | "base64url" | "hex" => None,
        other => Some(other),
    }
}

/// `buffer.isAscii(input)` — every byte `<= 0x7F`.
///
/// Inputs without a readable byte view raise `ERR_INVALID_ARG_TYPE`, matching
/// Node's contract instead of returning an ambiguous boolean or `undefined`.
extern "C" fn is_ascii(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(bytes) = bytes_argument(input) else {
        if entry::buffer_detached(input) {
            entry::invalid_state("Cannot use a detached ArrayBuffer");
        } else {
            entry::invalid_arg_instance("input", "Buffer, TypedArray, or DataView", input);
        }
        return entry::undefined_value();
    };
    entry::boolean_value(bytes.iter().all(|byte| byte.is_ascii()))
}
/// `buffer.isUtf8(input)` — the bytes decode as UTF-8.
extern "C" fn is_utf8(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(bytes) = bytes_argument(input) else {
        if entry::buffer_detached(input) {
            entry::invalid_state("Cannot use a detached ArrayBuffer");
        } else {
            entry::invalid_arg_instance("input", "Buffer, TypedArray, or DataView", input);
        }
        return entry::undefined_value();
    };
    entry::boolean_value(std::str::from_utf8(&bytes).is_ok())
}


/// The bytes of a `Buffer`/typed array/`DataView` argument. `None` for
/// anything else — including a raw `ArrayBuffer`, refused by name in the
/// module doc.
fn bytes_argument(input: u64) -> Option<Vec<u8>> {
    entry::with_runtime(|context| entry::bytes_of(context, input))
}

/// The legacy `buffer.SlowBuffer(size)` factory.
extern "C" fn slow_buffer_native(
    _e: u64,
    _this: u64,
    size: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let buffer = entry::with_runtime(|context| entry::buffer_class(context));
    let alloc_unsafe = entry::with_runtime(|context| {
        entry::get_member(context, buffer, "allocUnsafe")
    });
    let absent = entry::undefined_value();
    entry::call(alloc_unsafe, buffer, size, absent, absent, absent)
}

/// The getter half of the mutable `buffer.INSPECT_MAX_BYTES` property.
extern "C" fn inspect_max_bytes_get(
    _e: u64,
    _this: u64,
    _a0: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    entry::make_number(f64::from_bits(INSPECT_MAX_BYTES.load(Ordering::SeqCst)))
}

/// The setter validates Node's non-negative numeric limit before publishing it.
extern "C" fn inspect_max_bytes_set(
    _e: u64,
    _this: u64,
    value: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let Some(number) = entry::number_of(value) else {
        entry::invalid_arg_type("INSPECT_MAX_BYTES", "number", value);
        return entry::undefined_value();
    };
    if number.is_nan() || number < 0.0 {
        entry::out_of_range("INSPECT_MAX_BYTES", ">= 0", value);
        return entry::undefined_value();
    }
    INSPECT_MAX_BYTES.store(number.to_bits(), Ordering::SeqCst);
    entry::undefined_value()
}

/// `buffer.resolveObjectURL(id)` — always `undefined`, and totally correct
/// rather than a stub. See the module doc: nothing can mint the ids it
/// resolves, so nothing can be registered under one.
extern "C" fn resolve_object_url(_e: u64, _this: u64, _id: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}
