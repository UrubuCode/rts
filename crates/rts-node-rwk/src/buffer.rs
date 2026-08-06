//! `node:buffer` — what is left once `Buffer` itself is a real class.
//!
//! # What moved out
//!
//! `Buffer` — the constructor, its statics (`alloc`/`from`/`concat`/…) and
//! every instance method (`toString`/`write`/`slice`/the numeric family) — now
//! lives in `rts-core-rwk::entry::buffer`, reached here through
//! [`rts_core_rwk::entry::buffer_class`] rather than rebuilt. It is a real
//! `Uint8Array` subclass there: `Buffer.prototype.toString` is an own,
//! inherited method now, not a name this module refused. The codecs
//! (`utf8`/`hex`/`base64`/`base64url`/`latin1`/`utf16le`) moved with it, into
//! `rts-core-rwk::entry::buffer::codec`, and are gone from this file — two
//! decoders answering one name was the duplication moving `Buffer` was for.
//!
//! # What this module still owes `node:buffer`
//!
//! The module-level exports Node's `buffer` module has beside the class:
//! `atob`/`btoa` (binary-string ⇄ base64, kept here since they are not
//! `Buffer` methods), `constants`, `kMaxLength`, `kStringMaxLength`, and
//! `transcode`. All four still use the runtime's own codecs indirectly through
//! `bytes_of`/`make_bytes`/`text_of`, the same primitives `node:fs` reaches the
//! runtime through — this module never had its own byte store to begin with.

use rts_core_rwk::entry::{self, Context, Provided};

/// The namespace `node:buffer` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("atob", atob), ("btoa", btoa), ("transcode", transcode)];
    let namespace = entry::make_namespace(context, members);

    let buffer = entry::buffer_class(context);
    entry::put_member(context, namespace, "Buffer", buffer);

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
extern "C" fn transcode(_e: u64, _this: u64, source: u64, from_enc: u64, to_enc: u64, _d: u64) -> u64 {
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, source)).unwrap_or_default();
    let from_name = entry::text_of(from_enc).unwrap_or_default();
    let to_name = entry::text_of(to_enc).unwrap_or_default();
    let (Some(from_canon), Some(to_canon)) =
        (transcode_encoding(&from_name), transcode_encoding(&to_name))
    else {
        return entry::with_runtime(|context| entry::make_bytes(context, &bytes));
    };
    let text = entry::decode_bytes(&bytes, from_canon);
    let out = entry::encode_text(&text, to_canon).unwrap_or(bytes);
    entry::with_runtime(|context| entry::make_bytes(context, &out))
}

/// A [`transcode`] encoding name, restricted to Node's six there (no
/// base64/hex — see the reference's `TranscodeEncoding`).
fn transcode_encoding(name: &str) -> Option<&'static str> {
    match entry::canonical_encoding(name)? {
        "base64" | "base64url" | "hex" => None,
        other => Some(other),
    }
}
