//! `node:buffer` — the synchronous byte/text codec surface, against
//! `docs/reference/node/buffer.md`.
//!
//! # The shape decision this module takes
//!
//! This engine builds no class and no prototype from Rust yet — see the three
//! doc comments in `rts-core-rwk`'s `entry::modules` this module is written
//! against. There is no cell a native here could hang a `Buffer.prototype`
//! method from, and no shape marker that would make `instanceof Buffer` mean
//! anything.
//!
//! So `Buffer.from`/`Buffer.alloc`/`Buffer.allocUnsafe`/`Buffer.concat` all
//! answer a plain `Uint8Array` — built with
//! [`make_bytes`](rts_core_rwk::entry::make_bytes), the same primitive `node:fs`
//! already answers file contents through — and **every instance method Node
//! documents** (`buf.toString()`, `buf.write()`, `buf.slice()`, `buf.fill()`,
//! the 44 numeric read/write methods, …) is refused **by name**: this module
//! installs no `Buffer.prototype`, so `Uint8Array.prototype` is untouched and
//! `buf.toString()` on a value this module returns is inherited from
//! `Object.prototype` through the typed-array chain, not a decoder. A program
//! that calls it gets Node's answer for the wrong reason, which is why it is
//! named here rather than left for a caller to discover as a silent
//! behavioural difference.
//!
//! The alternative — hanging `toString`/`write`/`slice`/… directly on each
//! returned object as OWN properties, so the call succeeds — was rejected. It
//! is observable exactly like a fake is: `Object.keys(buf)` would report
//! `"toString"`, `"write"`, … as OWN enumerable keys, when a real `Buffer`'s
//! are inherited and invisible to `Object.keys`; `JSON.stringify(buf)` would
//! serialize the METHODS as data properties (this engine's `own_keys` walk
//! does not skip functions); and a shape carrying a different property list
//! per instance defeats the shape cache that makes property reads fast at
//! all. None of that is "costs nothing a program observes" — it is a second,
//! worse divergence pretending to fix the first. What waits on `#[rtse::class]`
//! landing for a `Buffer` prototype (see `add-builtin-class`) is instance
//! methods, not this module.
//!
//! What this module DOES give the instance-method half, so a caller is not
//! left with only the static half: `Buffer.byteLength`/`Buffer.compare`/
//! `Buffer.isBuffer`/`Buffer.isEncoding` are the STATIC forms of Node's own
//! API, present already; nothing here invents a static form of an
//! instance-only method Node itself never offered as one.
//!
//! # Codecs
//!
//! Written independently here (no dependency on `rts-std::crypto`'s base64/
//! hex, which this crate cannot reach — see the reference's §5.7): `utf8`
//! (`std::str`/`String`, native), `hex`, `base64`/`base64url` (hand-rolled,
//! no crate), `latin1`/`binary`/`ascii` (byte-for-byte, with the divergence
//! Node itself documents for `ascii` decode — high bit stripped), and
//! `utf16le`/`ucs2` (`char::encode_utf16`/`decode_utf16`, `std`).
//!
//! # Not implemented, by name
//!
//! - **Every instance method** — see above; a decision, not a gap to close
//!   later without also deciding where a `Buffer` prototype lives.
//! - **`Buffer.allocUnsafeSlow`**, **`Buffer.copyBytesFrom`** — both answer
//!   exactly what `Buffer.allocUnsafe`/a plain copy already answers here
//!   (no pool to bypass, no element-vs-byte gap when everything is already a
//!   `Uint8Array`); omitted rather than given a second name for the same
//!   thing `allocUnsafe`/`from` already do.
//! - **`Buffer.poolSize`**, the allocation pool — `allocUnsafe` zero-fills
//!   fresh memory here rather than drawing from a shared pool; Node's pooling
//!   is a GC-pressure optimization this engine's allocator makes moot.
//! - **`Blob`, `File`** — need a handle table of their own and a Promise-
//!   settle primitive this crate cannot reach (reference §5.7); not started.
//! - **`resolveObjectURL`** — needs a cross-module object-URL registry that
//!   does not exist yet (reference §7); would answer `undefined` always if
//!   added, which is exactly what omitting it also gives a caller.
//! - **`isAscii`/`isUtf8`** — straightforward over `bytes_of`, omitted from
//!   this pass to keep the file inside its ceiling; not a design gap.
//! - **`INSPECT_MAX_BYTES`**, **`util.inspect` integration** — no
//!   `console.log` hex-dump formatting exists to hook in this engine yet.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:buffer` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("alloc", alloc),
        ("allocUnsafe", alloc_unsafe),
        ("from", from),
        ("concat", concat),
        ("byteLength", byte_length),
        ("isBuffer", is_buffer),
        ("isEncoding", is_encoding),
        ("compare", compare),
        ("atob", atob),
        ("btoa", btoa),
        ("transcode", transcode),
    ];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);

    // A `Buffer` STATIC object, so `import { Buffer } from "node:buffer"` and
    // `Buffer.from(...)` reach the same functions as the namespace's own
    // top-level `from`/`alloc`/etc — built from the SAME `members` list, since
    // `Buffer`'s statics are a subset-that-happens-to-be-everything-but-the-
    // three-module-level functions, and two lists inviting drift is worse
    // than `atob`/`btoa`/`transcode` also (harmlessly) hanging off `Buffer`.
    let buffer = rts_core_rwk::entry::make_namespace(context, members);
    rts_core_rwk::entry::put_member(context, namespace, "Buffer", buffer);

    let constants = rts_core_rwk::entry::make_object(context);
    let max_length = rts_core_rwk::entry::make_number(MAX_LENGTH as f64);
    rts_core_rwk::entry::put_member(context, constants, "MAX_LENGTH", max_length);
    let max_string_length = rts_core_rwk::entry::make_number(MAX_STRING_LENGTH as f64);
    rts_core_rwk::entry::put_member(context, constants, "MAX_STRING_LENGTH", max_string_length);
    rts_core_rwk::entry::put_member(context, namespace, "constants", constants);

    let k_max_length = rts_core_rwk::entry::make_number(MAX_LENGTH as f64);
    rts_core_rwk::entry::put_member(context, namespace, "kMaxLength", k_max_length);
    let k_string_max_length = rts_core_rwk::entry::make_number(MAX_STRING_LENGTH as f64);
    rts_core_rwk::entry::put_member(context, namespace, "kStringMaxLength", k_string_max_length);

    namespace
}

/// This engine picks its own ceiling rather than copying a V8 build's — see
/// reference §7. `i32::MAX` is the widest byte count a proven-integer index
/// can represent exactly.
const MAX_LENGTH: i64 = i32::MAX as i64;

/// Same reasoning as [`MAX_LENGTH`] — this engine's UTF-8 `Str` cells have no
/// narrower ceiling of their own, so the two share one number.
const MAX_STRING_LENGTH: i64 = i32::MAX as i64;

/// `Buffer.alloc(size, fill?, encoding?)`. Zero-filled unless `fill` is
/// given. A negative/non-numeric `size` answers a zero-length buffer rather
/// than throwing — this module's natives cannot throw across the call
/// boundary, the same stand-in `node:fs` states.
extern "C" fn alloc(_e: u64, _this: u64, size: u64, fill: u64, encoding: u64, _d: u64) -> u64 {
    let length = size_arg(size);
    let absent = rts_core_rwk::entry::undefined_value();
    let filled = if fill == absent {
        vec![0u8; length]
    } else if let Some(text) = rts_core_rwk::entry::text_of(fill) {
        let enc = encoding_arg(encoding);
        let pattern = encode(&text, &enc).unwrap_or_default();
        repeat_to(&pattern, length)
    } else if let Some(bytes) = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::bytes_of(context, fill)) {
        repeat_to(&bytes, length)
    } else if let Some(byte) = rts_core_rwk::entry::number_of(fill) {
        vec![byte as u8; length]
    } else {
        vec![0u8; length]
    };
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &filled))
}

/// `Buffer.allocUnsafe(size)`.
///
/// No pooled/uninitialized memory here (see the "not implemented" note on
/// pooling) — this answers the same zero-filled bytes [`alloc`] would with
/// no `fill`, a strictly SAFER answer than Node's uninitialized memory.
extern "C" fn alloc_unsafe(_e: u64, _this: u64, size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let length = size_arg(size);
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &vec![0u8; length]))
}

/// `Buffer.from(source, encodingOrOffset?, length?)`. Dispatches on what
/// `source` is: an existing view (copied via `bytes_of`), a string (encoded
/// per `encodingOrOffset`, default `utf8`), or an array-like of numbers.
extern "C" fn from(_e: u64, _this: u64, source: u64, encoding_or_offset: u64, _length: u64, _d: u64) -> u64 {
    if let Some(bytes) = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::bytes_of(context, source)) {
        return rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &bytes));
    }
    if let Some(text) = rts_core_rwk::entry::text_of(source) {
        let enc = encoding_arg(encoding_or_offset);
        let bytes = encode(&text, &enc).unwrap_or_default();
        return rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &bytes));
    }
    if rts_core_rwk::entry::is_array(source) {
        let bytes = collect_array(source)
            .into_iter()
            .map(|value| rts_core_rwk::entry::number_of(value).unwrap_or(0.0) as u8)
            .collect::<Vec<u8>>();
        return rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &bytes));
    }
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &[]))
}

/// `Buffer.concat(list, totalLength?)`.
extern "C" fn concat(_e: u64, _this: u64, list: u64, total_length: u64, _c: u64, _d: u64) -> u64 {
    let mut joined = Vec::new();
    for element in collect_array(list) {
        if let Some(bytes) = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::bytes_of(context, element)) {
            joined.extend_from_slice(&bytes);
        }
    }
    if let Some(wanted) = rts_core_rwk::entry::number_of(total_length) {
        joined.resize(wanted.max(0.0) as usize, 0);
    }
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &joined))
}

/// `Buffer.byteLength(string, encoding?)`. String-source only — a
/// `Uint8Array`/`ArrayBuffer` source answers its own `.length` already, an
/// ordinary property read rather than a second entry point.
extern "C" fn byte_length(_e: u64, _this: u64, source: u64, encoding: u64, _c: u64, _d: u64) -> u64 {
    if let Some(bytes) = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::bytes_of(context, source)) {
        return rts_core_rwk::entry::make_number(bytes.len() as f64);
    }
    let text = rts_core_rwk::entry::text_of(source).unwrap_or_default();
    let enc = encoding_arg(encoding);
    let length = encode(&text, &enc).map(|bytes| bytes.len()).unwrap_or(0);
    rts_core_rwk::entry::make_number(length as f64)
}

/// `Buffer.isBuffer(obj)`. No `Buffer` tag distinct from `Uint8Array`'s own
/// exists here (see the module doc), so this answers whether `obj` is a
/// `Uint8Array` at all — every value this module hands back IS one.
extern "C" fn is_buffer(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::bytes_of(context, value)).is_some();
    rts_core_rwk::entry::boolean_value(held)
}

/// `Buffer.isEncoding(encoding)`. Never throws — unknown answers `false`.
extern "C" fn is_encoding(_e: u64, _this: u64, encoding: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = rts_core_rwk::entry::text_of(encoding).unwrap_or_default();
    let held = canonical_encoding(&text).is_some();
    rts_core_rwk::entry::boolean_value(held)
}

/// `Buffer.compare(buf1, buf2)`.
extern "C" fn compare(_e: u64, _this: u64, a: u64, b: u64, _c: u64, _d: u64) -> u64 {
    let a = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::bytes_of(context, a)).unwrap_or_default();
    let b = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::bytes_of(context, b)).unwrap_or_default();
    let result = match a.cmp(&b) {
        std::cmp::Ordering::Less => -1.0,
        std::cmp::Ordering::Equal => 0.0,
        std::cmp::Ordering::Greater => 1.0,
    };
    rts_core_rwk::entry::make_number(result)
}

/// `buffer.atob(data)` — base64 to a binary string, one code unit per byte.
/// Invalid base64 answers `""` (no-throw stand-in); real Node throws a
/// `DOMException` this engine has no primordial for (reference §7).
extern "C" fn atob(_e: u64, _this: u64, data: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = rts_core_rwk::entry::text_of(data).unwrap_or_default();
    let bytes = decode_base64(&text);
    let binary: String = bytes.iter().map(|&byte| byte as char).collect();
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, &binary))
}

/// `buffer.btoa(data)` — a binary string (`U+0000`-`U+00FF`) to base64. A
/// char above `U+00FF` truncates to its low byte — same no-throw stand-in.
extern "C" fn btoa(_e: u64, _this: u64, data: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = rts_core_rwk::entry::text_of(data).unwrap_or_default();
    let bytes: Vec<u8> = text.chars().map(|ch| ch as u32 as u8).collect();
    let encoded = encode_base64(&bytes, true);
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, &encoded))
}

/// `buffer.transcode(source, fromEnc, toEnc)`. Decodes to a `String`
/// intermediate with `fromEnc`'s codec, then re-encodes with `toEnc`'s —
/// restricted to Node's own six binary-text encodings (no base64/hex); an
/// unsupported name answers a copy of `source` unchanged rather than throwing.
extern "C" fn transcode(_e: u64, _this: u64, source: u64, from_enc: u64, to_enc: u64, _d: u64) -> u64 {
    let bytes = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::bytes_of(context, source)).unwrap_or_default();
    let from_name = rts_core_rwk::entry::text_of(from_enc).unwrap_or_default();
    let to_name = rts_core_rwk::entry::text_of(to_enc).unwrap_or_default();
    let (Some(from_canon), Some(to_canon)) = (transcode_encoding(&from_name), transcode_encoding(&to_name)) else {
        return rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &bytes));
    };
    let text = decode(&bytes, from_canon);
    let out = encode(&text, to_canon).unwrap_or(bytes);
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_bytes(context, &out))
}

/// A [`transcode`] encoding name, restricted to Node's six there (no
/// base64/hex — see the reference's `TranscodeEncoding`).
fn transcode_encoding(name: &str) -> Option<&'static str> {
    match canonical_encoding(name)? {
        "base64" | "base64url" | "hex" => None,
        other => Some(other),
    }
}

/// `size` argument to [`alloc`]/[`alloc_unsafe`] — non-numeric or negative
/// answers `0` rather than throwing, the same no-throw stand-in as elsewhere.
fn size_arg(value: u64) -> usize {
    rts_core_rwk::entry::number_of(value)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .map(|n| n.min(MAX_LENGTH as f64) as usize)
        .unwrap_or(0)
}

/// An `encoding` argument, defaulting to `"utf8"` for `undefined`.
fn encoding_arg(value: u64) -> String {
    if value == rts_core_rwk::entry::undefined_value() {
        return "utf8".to_owned();
    }
    rts_core_rwk::entry::text_of(value).unwrap_or_else(|| "utf8".to_owned())
}

/// A JS array's elements, via `.length` + indexed access — the same walk
/// `querystring::collect_array` does; kept as this module's own copy since
/// this crate has no shared spot between its own modules to hold one.
fn collect_array(array: u64) -> Vec<u64> {
    let absent = rts_core_rwk::entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length_key = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, "length"));
    let length_value = rts_core_rwk::entry::get_indexed(array, length_key);
    let length = rts_core_rwk::entry::number_of(length_value).map(|value| value as usize).unwrap_or(0);
    (0..length)
        .map(|index| rts_core_rwk::entry::get_indexed(array, rts_core_rwk::entry::make_number(index as f64)))
        .collect()
}

/// `pattern` repeated/truncated to exactly `length` bytes. An empty pattern
/// filling a non-zero length answers zeros rather than the exception Node
/// throws there — the no-throw stand-in again.
fn repeat_to(pattern: &[u8], length: usize) -> Vec<u8> {
    if pattern.is_empty() {
        return vec![0u8; length];
    }
    (0..length).map(|index| pattern[index % pattern.len()]).collect()
}

/// An encoding name, normalized to this module's canonical spelling —
/// case-insensitive, folding every alias onto one of the six codecs
/// [`encode`]/[`decode`] implement. `None` is exactly [`is_encoding`]'s `false`.
fn canonical_encoding(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "utf8" | "utf-8" => Some("utf8"),
        "ascii" => Some("ascii"),
        "latin1" | "binary" => Some("latin1"),
        "utf16le" | "utf-16le" | "ucs2" | "ucs-2" => Some("utf16le"),
        "base64" => Some("base64"),
        "base64url" => Some("base64url"),
        "hex" => Some("hex"),
        _ => None,
    }
}

/// A string encoded to bytes under the named encoding. `None` for a name
/// [`canonical_encoding`] does not recognize.
fn encode(text: &str, encoding: &str) -> Option<Vec<u8>> {
    match canonical_encoding(encoding)? {
        "utf8" => Some(text.as_bytes().to_vec()),
        "ascii" | "latin1" => Some(text.chars().map(|ch| ch as u32 as u8).collect()),
        "utf16le" => Some(encode_utf16le(text)),
        // `base64`/`base64url` are DECODE targets, not encode targets, for
        // `Buffer.from(string, encoding)` — Node treats the STRING as
        // already-encoded base64 there, so "encoding" a string INTO base64
        // bytes means decoding it, which is what `from` wants; both names
        // decode through the same permissive [`decode_base64`].
        "base64" | "base64url" => Some(decode_base64(text)),
        "hex" => Some(decode_hex(text)),
        _ => None,
    }
}

/// Bytes decoded to text under the named encoding — [`transcode`]'s
/// intermediate step. Falls back to lossy UTF-8 for an unrecognized name
/// (unreachable via [`transcode_encoding`], kept total rather than partial).
fn decode(bytes: &[u8], encoding: &str) -> String {
    match canonical_encoding(encoding) {
        Some("utf8") => String::from_utf8_lossy(bytes).into_owned(),
        Some("ascii") => bytes.iter().map(|&byte| (byte & 0x7F) as char).collect(),
        Some("latin1") => bytes.iter().map(|&byte| byte as char).collect(),
        Some("utf16le") => decode_utf16le(bytes),
        Some("base64") | Some("base64url") => encode_base64(bytes, matches!(canonical_encoding(encoding), Some("base64"))),
        Some("hex") => encode_hex(bytes),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// UTF-16LE encode: `char::encode_utf16` already emits surrogate pairs; this
/// only lays each `u16` out little-endian.
fn encode_utf16le(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// UTF-16LE decode. An odd trailing byte is dropped; an unpaired surrogate
/// becomes `U+FFFD`, matching Node's `ucs2`/`utf16le` decode.
fn decode_utf16le(bytes: &[u8]) -> String {
    let units = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
    char::decode_utf16(units).map(|result| result.unwrap_or('\u{FFFD}')).collect()
}

/// Hex decode. Stops at the first non-hex-digit byte (Node's
/// truncate-on-invalid-nibble) and drops a trailing odd digit with no pair —
/// matches `Buffer.from('1a7', 'hex')` → `<1a>`.
fn decode_hex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text
        .bytes()
        .take_while(|byte| byte.is_ascii_hexdigit())
        .collect();
    digits
        .chunks_exact(2)
        .map(|pair| {
            let hi = (pair[0] as char).to_digit(16).unwrap_or(0);
            let lo = (pair[1] as char).to_digit(16).unwrap_or(0);
            ((hi << 4) | lo) as u8
        })
        .collect()
}

/// Hex encode — `2 * bytes.len()` lowercase digits.
fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The RFC 4648 alphabet, standard variant; [`encode_base64`] swaps the last
/// two characters (`+`/`/` → `-`/`_`) for the URL-safe form.
const BASE64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 encode. `standard` selects `+`/`/` with `=` padding; `false` gives
/// `-`/`_` with no padding — Node's `base64url` **encode** exactly (decode
/// is permissive either way — see [`decode_base64`]).
fn encode_base64(bytes: &[u8], standard: bool) -> String {
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let indices = [
            b0 >> 2,
            ((b0 & 0x03) << 4) | (b1 >> 4),
            ((b1 & 0x0F) << 2) | (b2 >> 6),
            b2 & 0x3F,
        ];
        for (position, index) in indices.iter().enumerate() {
            let emit = position == 1 || (position == 2 && chunk.len() > 1) || (position == 3 && chunk.len() > 2);
            if position == 0 || emit {
                let ch = BASE64_ALPHABET[*index as usize] as char;
                out.push(match ch {
                    '+' if !standard => '-',
                    '/' if !standard => '_',
                    other => other,
                });
            } else if standard {
                out.push('=');
            }
        }
    }
    out
}

/// Base64/base64url decode, permissive per reference §4: whitespace is
/// ignored, both alphabets are accepted regardless of which name was asked
/// for, and padding is optional.
fn decode_base64(text: &str) -> Vec<u8> {
    let mut bits: u32 = 0;
    let mut count = 0u32;
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for ch in text.chars() {
        let value = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => ch as u32 - 'a' as u32 + 26,
            '0'..='9' => ch as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            '=' => break,
            ch if ch.is_whitespace() => continue,
            _ => continue,
        };
        bits = (bits << 6) | value;
        count += 1;
        if count == 4 {
            out.push((bits >> 16) as u8);
            out.push((bits >> 8) as u8);
            out.push(bits as u8);
            bits = 0;
            count = 0;
        }
    }
    match count {
        3 => {
            bits <<= 6;
            out.push((bits >> 16) as u8);
            out.push((bits >> 8) as u8);
        }
        2 => {
            bits <<= 12;
            out.push((bits >> 16) as u8);
        }
        _ => {}
    }
    out
}
