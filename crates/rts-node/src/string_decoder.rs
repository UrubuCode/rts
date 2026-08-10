//! `node:string_decoder` — `StringDecoder`, over
//! `docs/reference/node/string_decoder.md`.
//!
//! # What reuse-check found
//!
//! Searched `rts-cranelift` (`src/tags`, `src/shape`, `src/types`) for anything
//! holding a partial value across two calls, and `rts-core`'s `Context` and
//! `entry/` for a codec, a pending-bytes buffer or an encoding table. Nothing
//! answers "hold a partial multi-byte character across two calls" — that state
//! is this module's whole reason to exist. What DOES exist, and is reused
//! rather than re-derived, is the codec: `entry::decode_bytes`/`encode_base64`
//! (`entry/buffer/codec.rs`, what `Buffer.prototype.toString` uses) do
//! `ascii`/`latin1`/`hex` outright, do the base64 alphabet and padding exactly,
//! and pair UTF-16 surrogates through `char::decode_utf16`; this module only
//! adds the holdback bookkeeping around them. `utf8` and the `utf16le` holdback
//! POINT need their own boundary detection (`std::str::from_utf8`'s
//! `valid_up_to`/`error_len`, and a surrogate check) because no existing codec
//! exposes "how much of this is a whole character" — `decode` there is
//! lossy-and-total, exactly the wrong shape for a decoder that must NOT swallow
//! a trailing partial character. The alias table is deliberately NOT
//! `entry::canonical_encoding`: that one knows no default, while this class's
//! (spec §4) maps absent/empty to `utf8`. The instance shape follows `fs::dir`:
//! one shared prototype, a Rust-side `TABLE` keyed by a number the instance
//! carries.
//!
//! # How a partial character survives across two `write()` calls
//!
//! Each decoder's Rust-side state ([`Decoder`], keyed in [`TABLE`] by the
//! `__decoderId` an instance carries) holds `pending: Vec<u8>` — the tail bytes
//! of the last `write()` that were not yet a whole unit. Every `write` prepends
//! `pending` to the new input, decodes as much as is a whole character/unit,
//! and re-populates `pending` with what is left. `end()` runs the same decode
//! with `is_end: true`, turning anything still held into exactly one U+FFFD
//! (or, for base64, the final short group) and clearing `pending` — leaving the
//! instance reusable.
//!
//! # The one rule
//!
//! Every entry point here opens exactly ONE [`entry::with_runtime`] borrow and
//! does all of its work inside it, using only context-taking helpers
//! (`get_member`, `put_member`, `is_object`, `bytes_of`, `string_in`,
//! `make_string`, `make_buffer`, `number_of`, `decode_bytes`, `encode_base64`).
//! An earlier shape opened three borrows in sequence and read `__decoderId`
//! through the AMBIENT `get_indexed`; that was legal only because the borrows
//! happened not to nest — a property a later edit can silently break. One
//! borrow with no ambient call inside it cannot be broken that way.
//! [`with_table`] takes a plain `Mutex` and calls nothing in the runtime, so
//! holding it inside the borrow cannot re-enter.
//!
//! # Not implemented, by name
//!
//! `ERR_UNKNOWN_ENCODING` as its own class — `new StringDecoder('utf-9')` now
//! throws (`entry::throw_type_error`, per rule 8 of `rts-core`'s README),
//! but the class raised is `TypeError` rather than Node's own; that is the
//! only raise this crate can reach publicly (see `throw_type_error`'s doc).
//!
//! An argument to `write`/`end` that is neither a string nor a view over bytes
//! (a number, a plain object, an array) still decodes as NO bytes and answers
//! `""` rather than raising `ERR_INVALID_ARG_TYPE` — coercing such a value to
//! text would be a fabricated answer, `write(42)` returning `"42"` is the
//! coercion-as-type-test defect this crate refuses, and this pass's reported
//! sites did not name this one. Doing nothing and saying so is the honest
//! remainder for now.
//!
//! Disposal — there is no `FinalizationRegistry` this crate can drive from
//! Rust, so a decoder's table entry outlives the JS object that named it, the
//! same trade `fs::dir`'s `TABLE` already makes.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core::entry::{self, Context, Provided};

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

impl Encoding {
    /// The size of Node's `lastChar` scratch buffer for this family. Node
    /// exposes a FIXED-size `Buffer` rather than a right-sized one, and code
    /// that inspected `lastChar` read it against `lastNeed`, so the size is
    /// part of what it saw.
    fn scratch_len(self) -> usize {
        match self {
            Encoding::Utf8 | Encoding::Utf16Le => 4,
            Encoding::Base64 | Encoding::Base64Url => 3,
            Encoding::Ascii | Encoding::Latin1 | Encoding::Hex => 0,
        }
    }
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

const METHODS: &[(&str, Provided)] =
    &[("write", write_method), ("end", end_method), ("text", text_method)];

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
    let raw = entry::with_runtime(|context| {
        // `string_in`, not `text_of`: this asks WHAT the argument is. Node
        // accepts a string or nothing and rejects everything else, so a value
        // that is not a string must not be stringified into a name the alias
        // table might then recognize.
        entry::string_in(context, encoding_arg).unwrap_or_default()
    });
    // An unrecognized name raises — see the module doc for why the class is
    // `TypeError` rather than Node's own `ERR_UNKNOWN_ENCODING`.
    let Some((encoding, canonical)) = normalize(&raw) else {
        entry::throw_type_error(&format!("Unknown encoding: {raw}"));
        return entry::undefined_value();
    };
    entry::with_runtime(|context| {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        with_table(|table| {
            table.insert(id, Decoder { encoding, pending: Vec::new() });
        });
        let prototype = entry::make_prototype(context, "StringDecoder", METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__decoderId", id_value);
        let encoding_value = entry::make_string(context, canonical);
        entry::put_member(context, instance, "encoding", encoding_value);
        publish_state(context, instance, encoding, &[]);
        instance
    })
}

/// The instance's table key, read with the context already in hand.
fn id_of(context: &mut Context, this: u64) -> Option<u64> {
    let value = entry::get_member(context, this, "__decoderId");
    entry::number_of(value).map(|value| value as u64)
}

/// An argument's bytes, with the context already in hand.
///
/// A view over bytes (`Buffer`/`TypedArray`/`DataView`) reads through
/// [`entry::bytes_of`]; a JS string is re-encoded UTF-8, matching Node's
/// implicit `Buffer.from(buffer)` (spec §4) — regardless of the decoder's OWN
/// target encoding. Anything else answers no bytes; the module doc says why.
fn bytes_in(context: &Context, value: u64) -> Vec<u8> {
    if let Some(text) = entry::string_in(context, value) {
        return text.into_bytes();
    }
    entry::bytes_of(context, value).unwrap_or_default()
}

/// `decoder.write(buffer)`.
extern "C" fn write_method(_e: u64, this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let bytes = bytes_in(context, buffer);
        step(context, this, bytes, false)
    })
}

/// `decoder.end(buffer?)`.
extern "C" fn end_method(_e: u64, this: u64, buffer: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let bytes = if buffer == absent { Vec::new() } else { bytes_in(context, buffer) };
        step(context, this, bytes, true)
    })
}

/// `decoder.text(buffer, offset)` — the legacy/undocumented dispatcher.
///
/// A stateless preview: decodes `buffer` from `offset` against the instance's
/// OWN encoding, flushing any trailing partial unit, and never touches
/// [`Decoder::pending`]. Node's version mutates `lastNeed`/`lastChar` as a side
/// effect; reproducing that would let an undocumented entry point corrupt a
/// documented one's in-flight sequence — a worse trade than the divergence.
extern "C" fn text_method(_e: u64, this: u64, buffer: u64, offset: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let bytes = bytes_in(context, buffer);
        let start = entry::number_of(offset).unwrap_or(0.0).max(0.0) as usize;
        let slice = bytes.get(start..).unwrap_or(&[]).to_vec();
        let encoding = id_of(context, this)
            .and_then(|id| with_table(|table| table.get(&id).map(|decoder| decoder.encoding)))
            .unwrap_or(Encoding::Utf8);
        let (text, _leftover) = decode_chunk(encoding, &[], &slice, true);
        entry::make_string(context, &text)
    })
}

/// The shared core of `write`/`end`: locates the instance's [`Decoder`],
/// decodes `input` against its held-back `pending`, stores whatever is still
/// incomplete back (or clears it, at `is_end`), republishes the legacy
/// `lastNeed`/`lastTotal`/`lastChar` view of that state, and answers the
/// decoded text as a value.
fn step(context: &mut Context, this: u64, input: Vec<u8>, is_end: bool) -> u64 {
    let Some(id) = id_of(context, this) else {
        return entry::make_string(context, "");
    };
    let decoded = with_table(|table| {
        let decoder = table.get_mut(&id)?;
        let (text, leftover) = decode_chunk(decoder.encoding, &decoder.pending, &input, is_end);
        decoder.pending = leftover;
        Some((text, decoder.encoding, decoder.pending.clone()))
    });
    let Some((text, encoding, pending)) = decoded else {
        return entry::make_string(context, "");
    };
    publish_state(context, this, encoding, &pending);
    entry::make_string(context, &text)
}

/// Node's `lastNeed`/`lastTotal`/`lastChar` for the bytes currently held back.
///
/// Refreshed as plain data properties after every state change rather than
/// served by accessors, because this crate can install a method on a prototype
/// but not a getter — and a method named `lastNeed` would answer a FUNCTION
/// where Node answers a number, which the code that reads these (older
/// `readable-stream` forks) would trip on immediately.
fn publish_state(context: &mut Context, instance: u64, encoding: Encoding, pending: &[u8]) {
    let (need, total) = pending_shape(encoding, pending);
    let need_value = entry::make_number(need as f64);
    entry::put_member(context, instance, "lastNeed", need_value);
    let total_value = entry::make_number(total as f64);
    entry::put_member(context, instance, "lastTotal", total_value);
    let mut scratch = vec![0u8; encoding.scratch_len().max(pending.len())];
    scratch[..pending.len()].copy_from_slice(pending);
    let scratch_value = entry::make_buffer(context, &scratch);
    entry::put_member(context, instance, "lastChar", scratch_value);
}

/// How many more bytes the held-back sequence needs, and how long it will be
/// once whole. DERIVED from the pending bytes rather than stored beside them,
/// so the two cannot disagree; Node keeps separate counters only because its
/// state machine writes them before it has the bytes.
fn pending_shape(encoding: Encoding, pending: &[u8]) -> (usize, usize) {
    if pending.is_empty() {
        return (0, 0);
    }
    match encoding {
        Encoding::Utf8 => {
            let lead = pending[0];
            let total = match lead {
                0xF0..=0xF7 => 4,
                0xE0..=0xEF => 3,
                0xC0..=0xDF => 2,
                // A held tail always starts on a lead byte, so this is
                // unreachable through `write`; answering the length it has is
                // the only claim that stays true if it is ever reached.
                _ => pending.len(),
            };
            (total.saturating_sub(pending.len()), total)
        }
        // One dangling byte needs its partner; a whole dangling high surrogate
        // needs the two bytes of the low surrogate that would pair with it.
        Encoding::Utf16Le if pending.len() == 1 => (1, 2),
        Encoding::Utf16Le => (2, 4),
        Encoding::Base64 | Encoding::Base64Url => (3 - pending.len(), 3),
        Encoding::Ascii | Encoding::Latin1 | Encoding::Hex => (0, 0),
    }
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
/// invalid" (`error_len() == Some(n)`, one U+FFFD, resume after the bad bytes)
/// from "incomplete at the end" (`error_len() == None`, hold the tail), which
/// is exactly the split Node's own state machine computes by hand — see the
/// spec §4/§5.1. The `Some(n)` arm is what makes a bad continuation byte fail
/// EAGERLY: `C2 41` answers `"\u{FFFD}A"` on the same `write`.
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
                    // ONE replacement character per rejected sequence, not one
                    // per byte — the v8.0.0 semantics change in spec §2.1, and
                    // what `error_len` already groups for us.
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

/// `utf16le`/`ucs2` — pair bytes into code units and decode through
/// [`entry::decode_bytes`], whose `char::decode_utf16` already combines a
/// surrogate PAIR and turns an unpaired surrogate into U+FFFD. What this adds
/// is the holdback: a dangling odd byte, and — the case the codec alone gets
/// wrong — a trailing HIGH surrogate, whose low partner may be the first two
/// bytes of the next chunk. Decoding it now would answer U+FFFD for a
/// character about to arrive intact.
fn decode_utf16le(pending: &[u8], input: &[u8], is_end: bool) -> (String, Vec<u8>) {
    let buffer = joined(pending, input);
    let paired_len = buffer.len() - (buffer.len() % 2);
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
        // Whether the remainder is a lone byte or a whole unpaired high
        // surrogate, it is exactly ONE U+FFFD — spec §4, family 2.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The boundary cases spec §6 names, at the byte level — the only layer
    /// testable without a running program, and where the two defects that
    /// matter (a swallowed partial character, a fabricated replacement) live.
    #[test]
    fn a_split_multi_byte_sequence_survives_the_boundary() {
        // '😀' is F0 9F 98 80; every one of the three splits must hold back.
        let emoji = "😀".as_bytes();
        for split in 1..4 {
            let (first, held) = decode_utf8(&[], &emoji[..split], false);
            assert_eq!(first, "");
            assert_eq!(held.len(), split);
            let (second, rest) = decode_utf8(&held, &emoji[split..], false);
            assert_eq!(second, "😀");
            assert!(rest.is_empty());
        }
    }

    #[test]
    fn a_bad_continuation_byte_fails_immediately_rather_than_waiting() {
        let (text, held) = decode_utf8(&[], &[0xC2, 0x41], false);
        assert_eq!(text, "\u{FFFD}A");
        assert!(held.is_empty());
    }

    #[test]
    fn an_incomplete_sequence_flushes_as_exactly_one_replacement() {
        let (_, held) = decode_utf8(&[], &[0xE2], false);
        let (text, rest) = decode_utf8(&held, &[], true);
        assert_eq!(text, "\u{FFFD}");
        assert!(rest.is_empty());
    }

    #[test]
    fn a_surrogate_pair_split_between_its_two_units_recombines() {
        let astral: Vec<u8> = "𝌆".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let (first, held) = decode_utf16le(&[], &astral[..2], false);
        assert_eq!(first, "");
        assert_eq!(held, astral[..2]);
        let (second, rest) = decode_utf16le(&held, &astral[2..], false);
        assert_eq!(second, "𝌆");
        assert!(rest.is_empty());
    }

    #[test]
    fn an_odd_trailing_byte_is_held_and_paired_with_the_next_chunk() {
        let bytes: Vec<u8> = "hi".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let (first, held) = decode_utf16le(&[], &bytes[..3], false);
        assert_eq!(first, "h");
        assert_eq!(held, bytes[2..3]);
        let (second, _) = decode_utf16le(&held, &bytes[3..], false);
        assert_eq!(second, "i");
    }

    #[test]
    fn a_dangling_high_surrogate_ends_as_one_replacement() {
        let astral: Vec<u8> = "𝌆".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let (_, held) = decode_utf16le(&[], &astral[..2], false);
        let (text, rest) = decode_utf16le(&held, &[], true);
        assert_eq!(text, "\u{FFFD}");
        assert!(rest.is_empty());
    }

    #[test]
    fn base64_holds_back_to_a_three_byte_group_and_pads_only_at_the_end() {
        let (first, held) = decode_base64_family(&[], &[1, 2, 3, 4], false, true);
        assert_eq!(first, entry::encode_base64(&[1, 2, 3], true));
        assert_eq!(held, vec![4]);
        let (last, _) = decode_base64_family(&held, &[], true, true);
        assert!(last.ends_with("=="));
        assert_eq!(format!("{first}{last}"), entry::encode_base64(&[1, 2, 3, 4], true));
        let (url, _) = decode_base64_family(&[4], &[], true, false);
        assert!(!url.contains('='));
    }

    #[test]
    fn the_legacy_counters_describe_the_bytes_actually_held() {
        assert_eq!(pending_shape(Encoding::Utf8, &[0xE2, 0x82]), (1, 3));
        assert_eq!(pending_shape(Encoding::Utf16Le, &[0x00]), (1, 2));
        assert_eq!(pending_shape(Encoding::Utf16Le, &[0x34, 0xD8]), (2, 4));
        assert_eq!(pending_shape(Encoding::Base64, &[1]), (2, 3));
        assert_eq!(pending_shape(Encoding::Hex, &[1]), (0, 0));
        assert_eq!(pending_shape(Encoding::Utf8, &[]), (0, 0));
    }
}
