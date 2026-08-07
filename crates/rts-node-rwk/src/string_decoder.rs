//! `node:string_decoder` — `StringDecoder`, over
//! `docs/reference/node/string_decoder.md`.
//!
//! # What reuse-check found
//!
//! Nothing answers "hold a partial multi-byte character across two calls" —
//! that state is this module's whole reason to exist, and no table in
//! `rts-cranelift` or `rts-core-rwk` tracks it. What DOES already exist, and
//! is reused rather than re-derived, is the byte↔text codec itself:
//! `rts_core_rwk::entry::decode_bytes`/`encode_base64` (the same functions
//! `node:buffer`'s `Buffer` uses) do the "simple" encodings (`ascii`,
//! `latin1`, `hex`) outright and the base64 grouping arithmetic exactly —
//! this module only adds the byte-holdback bookkeeping around them. `utf8`
//! and `utf16le` need their own boundary detection (`std::str::from_utf8`'s
//! `valid_up_to`/`error_len`, and a hand-rolled surrogate check) because no
//! existing codec exposes "how much of this is a whole character" — `decode`
//! there is lossy-and-total, which is exactly the wrong shape for a decoder
//! that must NOT swallow a trailing partial character. The instance/handle
//! shape follows `fs::dirent`/`fs::dir`: one shared prototype via
//! `make_prototype`, a small Rust-side table keyed by a number the instance
//! carries, following `fs::dir`'s `TABLE`/`with_table` pattern exactly.
//!
//! # How a partial character survives across two `write()` calls
//!
//! Each decoder's Rust-side state ([`Decoder`], keyed in [`TABLE`] by the
//! `__decoderId` an instance carries) holds `pending: Vec<u8>` — the tail
//! bytes of the last `write()` that were not yet a whole unit. Every `write`
//! prepends `pending` to the new input before decoding, decodes as much as is
//! a whole character/unit, and re-populates `pending` with whatever is left
//! at the end. `end()` runs the same decode with `is_end: true`, which turns
//! any bytes still held into exactly one U+FFFD (or, for `ascii`/`latin1`/
//! `hex`/base64 families, the family's own end-of-stream rule) instead of
//! holding them again, and clears `pending` — leaving the instance reusable.
//!
//! # The one rule
//!
//! [`entry::with_runtime`] is used only to build/read a value; the byte-level
//! decode functions below never call an ambient (ungated) `entry::*` helper
//! from inside one — see [`bytes_of_argument`] and [`finish`] for where the
//! borrow opens and closes.
//!
//! # Why `write`/`end` never throw
//!
//! A native entry point here cannot raise a catchable JS exception — see
//! `crate::events`'s module doc for the same limit, hit first there. Node's
//! `write`/`end` never throw either (malformed input degrades to U+FFFD), so
//! that limit costs nothing on the hot path. The one place Node DOES throw —
//! an unrecognized `encoding` name, `ERR_UNKNOWN_ENCODING` — has no honest
//! native answer here either, so [`normalize`] falls back to `utf8` rather
//! than fabricating a throw, the same no-throw stand-in `node:buffer`'s
//! `atob`/`transcode` already use for their own unrepresentable-error cases.
//!
//! # Not implemented, by name
//!
//! `lastChar`/`lastNeed`/`lastTotal` — the legacy/undocumented getters (spec
//! §5.8 phase e); no consumer in this repository's corpus has been found to
//! need them over the documented `write`/`end` surface. `ERR_UNKNOWN_ENCODING`
//! — see above; an unknown encoding name silently normalizes to `utf8`
//! instead of throwing. Disposal — there is no `FinalizationRegistry` this
//! crate can drive from Rust, so a decoder's table entry outlives the JS
//! object that named it, the same trade `fs::dir`'s `TABLE` already makes.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core_rwk::entry::{self, Context, Provided};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Utf8,
    Utf16Le,
    Base64,
    Base64Url,
    Ascii,
    Latin1,
    Hex,
}

struct Decoder {
    encoding: Encoding,
    pending: Vec<u8>,
}

static TABLE: Mutex<Option<HashMap<u64, Decoder>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, Decoder>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[("write", write_method), ("end", end_method), ("text", text_method)];

/// The namespace `node:string_decoder` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("StringDecoder", construct)];
    let namespace = entry::make_namespace(context, members);
    let prototype = entry::make_prototype(context, "StringDecoder", METHODS);
    let constructor = entry::get_member(context, namespace, "StringDecoder");
    entry::put_member(context, constructor, "prototype", prototype);
    namespace
}

/// `encoding` (case-insensitive, alias-folded) to its family and canonical
/// spelling. `None` for a name none of the seven recognize.
fn normalize(name: &str) -> Option<(Encoding, &'static str)> {
    match name.to_ascii_lowercase().as_str() {
        "" | "utf8" | "utf-8" => Some((Encoding::Utf8, "utf8")),
        "ucs2" | "ucs-2" | "utf16le" | "utf-16le" => Some((Encoding::Utf16Le, "utf16le")),
        "latin1" | "binary" => Some((Encoding::Latin1, "latin1")),
        "base64" => Some((Encoding::Base64, "base64")),
        "base64url" => Some((Encoding::Base64Url, "base64url")),
        "ascii" => Some((Encoding::Ascii, "ascii")),
        "hex" => Some((Encoding::Hex, "hex")),
        _ => None,
    }
}

/// `new StringDecoder(encoding?)` — also works called plainly, same
/// `is_object`-on-`this` pattern `events::make_emitter` uses.
extern "C" fn construct(_e: u64, this: u64, encoding_arg: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let raw = if encoding_arg == absent { String::new() } else { entry::text_of(encoding_arg).unwrap_or_default() };
    // An unrecognized name falls back to `utf8` — see the module doc for why
    // this cannot be the `ERR_UNKNOWN_ENCODING` throw the real class raises.
    let (encoding, canonical) = normalize(&raw).unwrap_or((Encoding::Utf8, "utf8"));
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| {
        table.insert(id, Decoder { encoding, pending: Vec::new() });
    });
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "StringDecoder", METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__decoderId", id_value);
        let encoding_value = entry::make_string(context, canonical);
        entry::put_member(context, instance, "encoding", encoding_value);
        instance
    })
}

fn id_of(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, string_key("__decoderId"));
    entry::number_of(value).map(|value| value as u64)
}

fn string_key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// An argument's bytes: an object (`Buffer`/`TypedArray`/`DataView`) reads
/// through [`entry::bytes_of`]; a JS string is re-encoded UTF-8, matching
/// Node's own implicit `Buffer.from(buffer)` for the string overload (§4 of
/// the spec) — regardless of the decoder's OWN configured target encoding.
/// `entry::is_object`/`entry::bytes_of`/[`entry::text_in`](entry::text_in)
/// are all context-taking, so this whole read happens inside one borrow.
fn bytes_of_argument(value: u64) -> Vec<u8> {
    let absent = entry::undefined_value();
    if value == absent {
        return Vec::new();
    }
    entry::with_runtime(|context| {
        if entry::is_object(context, value) {
            entry::bytes_of(context, value).unwrap_or_default()
        } else {
            entry::text_in(context, value).map(String::into_bytes).unwrap_or_default()
        }
    })
}

/// `decoder.write(buffer)`.
extern "C" fn write_method(_e: u64, this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    finish(run(this, bytes_of_argument(buffer), false))
}

/// `decoder.end(buffer?)`.
extern "C" fn end_method(_e: u64, this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let bytes = if buffer == absent { Vec::new() } else { bytes_of_argument(buffer) };
    finish(run(this, bytes, true))
}

/// `decoder.text(buffer, offset)` — the legacy/undocumented dispatcher.
/// Approximated as a stateless preview: decodes `buffer` from `offset`
/// against the instance's OWN encoding, forcing a flush of any trailing
/// partial unit, but never touches [`Decoder::pending`] — real Node's
/// version is this class's internal building block for `write`, not a pure
/// function, and reproducing that coupling exactly is not worth it for an
/// undocumented entry point (see the module doc).
extern "C" fn text_method(_e: u64, this: u64, buffer: u64, offset: u64, _a2: u64, _a3: u64) -> u64 {
    let bytes = bytes_of_argument(buffer);
    let start = entry::number_of(offset).unwrap_or(0.0).max(0.0) as usize;
    let slice = bytes.get(start..).unwrap_or(&[]);
    let encoding = id_of(this)
        .and_then(|id| with_table(|table| table.get(&id).map(|decoder| decoder.encoding)))
        .unwrap_or(Encoding::Utf8);
    let (text, _leftover) = decode_chunk(encoding, &[], slice, true);
    entry::with_runtime(|context| entry::make_string(context, &text))
}

/// The shared core of `write`/`end`: locates the instance's [`Decoder`],
/// decodes `input` against its held-back `pending`, and stores whatever is
/// still incomplete back — or clears it, at `is_end`.
fn run(this: u64, input: Vec<u8>, is_end: bool) -> String {
    let Some(id) = id_of(this) else {
        return String::new();
    };
    with_table(|table| {
        let Some(decoder) = table.get_mut(&id) else {
            return String::new();
        };
        let (text, leftover) = decode_chunk(decoder.encoding, &decoder.pending, &input, is_end);
        decoder.pending = leftover;
        text
    })
}

/// A decoded string, as a value — the one place [`run`]'s output crosses
/// back into the runtime.
fn finish(text: String) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, &text))
}

/// One family's decode step: `pending` (held back from the last call) plus
/// `input`, split into the decodable prefix and whatever is still an
/// incomplete unit — empty at `is_end`, since an incomplete unit is flushed
/// as replacement character(s) there instead of held again.
fn decode_chunk(encoding: Encoding, pending: &[u8], input: &[u8], is_end: bool) -> (String, Vec<u8>) {
    match encoding {
        Encoding::Utf8 => decode_utf8(pending, input, is_end),
        Encoding::Utf16Le => decode_utf16le(pending, input, is_end),
        Encoding::Base64 => decode_base64_family(pending, input, is_end, true),
        Encoding::Base64Url => decode_base64_family(pending, input, is_end, false),
        Encoding::Ascii => (entry::decode_bytes(&joined(pending, input), "ascii"), Vec::new()),
        Encoding::Latin1 => (entry::decode_bytes(&joined(pending, input), "latin1"), Vec::new()),
        Encoding::Hex => (entry::decode_bytes(&joined(pending, input), "hex"), Vec::new()),
    }
}

fn joined(pending: &[u8], input: &[u8]) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(pending.len() + input.len());
    buffer.extend_from_slice(pending);
    buffer.extend_from_slice(input);
    buffer
}

/// `utf8` — `std::str::from_utf8`'s error already distinguishes "genuinely
/// invalid" (`error_len() == Some(n)`, one U+FFFD, resume after the bad
/// bytes) from "incomplete at the end" (`error_len() == None`, hold the
/// tail), which is exactly the split Node's own state machine computes by
/// hand — see the spec §4/§5.1.
fn decode_utf8(pending: &[u8], input: &[u8], is_end: bool) -> (String, Vec<u8>) {
    let buffer = joined(pending, input);
    let mut out = String::new();
    let mut rest: &[u8] = &buffer;
    loop {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                out.push_str(valid);
                return (out, Vec::new());
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                // SAFETY: `valid_up_to` is exactly the length `from_utf8`
                // itself verified as valid UTF-8 for this slice.
                out.push_str(unsafe { std::str::from_utf8_unchecked(&rest[..valid_up_to]) });
                match error.error_len() {
                    Some(bad_len) => {
                        out.push('\u{FFFD}');
                        rest = &rest[valid_up_to + bad_len..];
                    }
                    None => {
                        let tail = &rest[valid_up_to..];
                        if is_end {
                            if !tail.is_empty() {
                                out.push('\u{FFFD}');
                            }
                            return (out, Vec::new());
                        }
                        return (out, tail.to_vec());
                    }
                }
            }
        }
    }
}

/// `utf16le`/`ucs2` — pair bytes into code units, decode with
/// [`char::decode_utf16`] (which already turns an unpaired surrogate into
/// U+FFFD), and hold back either a dangling odd byte or a dangling
/// high-surrogate pair rather than decoding it — see the spec §4.
fn decode_utf16le(pending: &[u8], input: &[u8], is_end: bool) -> (String, Vec<u8>) {
    let buffer = joined(pending, input);
    let paired_len = buffer.len() - (buffer.len() % 2);
    // How many trailing bytes of the *paired* prefix to hold back: 0, unless
    // the last complete code unit is an unpaired high surrogate, in which
    // case its own 2 bytes are held so a following low surrogate can pair
    // with it.
    let mut safe_len = paired_len;
    if paired_len >= 2 {
        let last_unit = u16::from_le_bytes([buffer[paired_len - 2], buffer[paired_len - 1]]);
        if (0xD800..=0xDBFF).contains(&last_unit) {
            safe_len -= 2;
        }
    }
    let mut out = entry::decode_bytes(&buffer[..safe_len], "utf16le");
    let mut leftover = buffer[safe_len..].to_vec();
    if is_end {
        if !leftover.is_empty() {
            out.push('\u{FFFD}');
        }
        leftover.clear();
    }
    (out, leftover)
}

/// `base64`/`base64url` — 3-byte groups; `entry::encode_base64` already pads
/// (or doesn't, per `standard`) based on the LAST chunk's own length, so
/// handing it exactly the held-back remainder at `end` produces the correct
/// final short group with no extra logic here.
fn decode_base64_family(pending: &[u8], input: &[u8], is_end: bool, standard: bool) -> (String, Vec<u8>) {
    let buffer = joined(pending, input);
    if is_end {
        return (entry::encode_base64(&buffer, standard), Vec::new());
    }
    let usable_len = buffer.len() - (buffer.len() % 3);
    let text = entry::encode_base64(&buffer[..usable_len], standard);
    (text, buffer[usable_len..].to_vec())
}
