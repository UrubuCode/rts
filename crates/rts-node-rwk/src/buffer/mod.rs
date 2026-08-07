//! `node:buffer` — what is left once `Buffer` itself is a real class.
//!
//! # What reuse-check found
//!
//! Everything byte-shaped here already has ONE implementation and this module
//! calls it rather than growing a second:
//!
//! - The `Buffer` class — constructor, statics, every instance method and the
//!   six codecs — lives in `rts-core-rwk::entry::buffer`, reached through
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
//!   `rts-core-rwk`'s buffer table, which differs because it backs a MUTABLE
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
//!   named here rather than silently changing `blob.size`. The same limit
//!   makes [`is_ascii`]/[`is_utf8`] answer `undefined` for an `ArrayBuffer`
//!   argument instead of guessing. The missing host capability is "bytes of an
//!   `ArrayBuffer` cell".
//! - **`resolveObjectURL(id)`** answers `undefined` for every `id`, and that is
//!   a correct total answer rather than a stub: the ids it resolves are minted
//!   only by `URL.createObjectURL`, which `node:url` refuses by name for want
//!   of a shared registry (reference §5.7/§7). Nothing can mint one, so
//!   nothing can be found.
//! - **The `Blob`/`File`/`atob`/`btoa` ambient globals.** They are members of
//!   this namespace and reachable by import; binding them as globals happens in
//!   `lib.rs`, which this module does not own.
//! - **Every `ERR_*` throw** in the reference's error column. A native entry
//!   point here cannot raise a catchable JS exception — the limit `events` and
//!   `string_decoder` already state — so `atob` on invalid base64, `btoa` above
//!   `U+00FF`, and `transcode` with an unsupported encoding each take the
//!   no-throw stand-in documented on the function itself.
//! - **`blob.size`/`blob.type`/`file.name`/`file.lastModified` are own data
//!   properties, not read-only accessors**, so a program can assign to them.
//!   The same trade a typed array's `length` already makes in `rts-core-rwk`.

mod blob;

use rts_core_rwk::entry::{self, Context, Provided};

/// The namespace `node:buffer` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("atob", atob),
        ("btoa", btoa),
        ("transcode", transcode),
        ("isAscii", is_ascii),
        ("isUtf8", is_utf8),
        ("resolveObjectURL", resolve_object_url),
    ];
    let namespace = entry::make_namespace(context, members);

    let buffer = entry::buffer_class(context);
    entry::put_member(context, namespace, "Buffer", buffer);
    blob::install(context, namespace);

    let constants = entry::make_object(context);
    let max_length = entry::make_number(MAX_LENGTH as f64);
    entry::put_member(context, constants, "MAX_LENGTH", max_length);
    let max_string_length = entry::make_number(MAX_STRING_LENGTH as f64);
    entry::put_member(context, constants, "MAX_STRING_LENGTH", max_string_length);
    entry::put_member(context, namespace, "constants", constants);

    let k_max_length = entry::make_number(MAX_LENGTH as f64);
    entry::put_member(context, namespace, "kMaxLength", k_max_length);
    let k_string_max_length = entry::make_number(MAX_STRING_LENGTH as f64);
    entry::put_member(context, namespace, "kStringMaxLength", k_string_max_length);
    // Mutable in Node, and mutable here for free: an own data property on the
    // namespace object is what `buffer.INSPECT_MAX_BYTES = 10` assigns to.
    // Nothing in this engine READS it yet — `util.inspect` has its own limit —
    // so it is a value a program can round-trip rather than a knob, which is
    // what the reference §2.3 documents it as.
    let inspect_max_bytes = entry::make_number(50.0);
    entry::put_member(context, namespace, "INSPECT_MAX_BYTES", inspect_max_bytes);

    namespace
}

/// This engine picks its own ceiling rather than copying a V8 build's — see
/// reference §7. `i32::MAX` is the widest byte count a proven-integer index
/// can represent exactly.
const MAX_LENGTH: i64 = i32::MAX as i64;

/// Same reasoning as [`MAX_LENGTH`] — this engine's UTF-8 `Str` cells have no
/// narrower ceiling of their own, so the two share one number.
const MAX_STRING_LENGTH: i64 = i32::MAX as i64;

/// `buffer.atob(data)` — base64 to a binary string, one code unit per byte.
/// Invalid base64 answers `""` (no-throw stand-in); real Node throws a
/// `DOMException` this engine has no primordial for (reference §7).
extern "C" fn atob(_e: u64, _this: u64, data: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = entry::text_of(data).unwrap_or_default();
    let bytes = entry::decode_base64(&text);
    let binary: String = bytes.iter().map(|&byte| byte as char).collect();
    entry::with_runtime(|context| entry::make_string(context, &binary))
}

/// `buffer.btoa(data)` — a binary string (`U+0000`-`U+00FF`) to base64. A
/// char above `U+00FF` truncates to its low byte — same no-throw stand-in.
extern "C" fn btoa(_e: u64, _this: u64, data: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = entry::text_of(data).unwrap_or_default();
    let bytes: Vec<u8> = text.chars().map(|ch| ch as u32 as u8).collect();
    let encoded = entry::encode_base64(&bytes, true);
    entry::with_runtime(|context| entry::make_string(context, &encoded))
}

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
/// `undefined`, not `false`, for an argument whose bytes cannot be read: Node
/// throws `ERR_INVALID_ARG_TYPE` there, and `false` would be a wrong answer
/// that runs — indistinguishable from a real buffer containing a high byte.
extern "C" fn is_ascii(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    match bytes_argument(input) {
        Some(bytes) => entry::boolean_value(bytes.iter().all(|byte| byte.is_ascii())),
        None => entry::undefined_value(),
    }
}

/// `buffer.isUtf8(input)` — the bytes decode as UTF-8. Same `undefined`-for-
/// unreadable rule as [`is_ascii`].
extern "C" fn is_utf8(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    match bytes_argument(input) {
        Some(bytes) => entry::boolean_value(std::str::from_utf8(&bytes).is_ok()),
        None => entry::undefined_value(),
    }
}

/// The bytes of a `Buffer`/typed array/`DataView` argument. `None` for
/// anything else — including a raw `ArrayBuffer`, refused by name in the
/// module doc.
fn bytes_argument(input: u64) -> Option<Vec<u8>> {
    entry::with_runtime(|context| entry::bytes_of(context, input))
}

/// `buffer.resolveObjectURL(id)` — always `undefined`, and totally correct
/// rather than a stub. See the module doc: nothing can mint the ids it
/// resolves, so nothing can be registered under one.
extern "C" fn resolve_object_url(_e: u64, _this: u64, _id: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}
