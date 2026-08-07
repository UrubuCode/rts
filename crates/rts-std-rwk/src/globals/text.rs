//! `TextEncoder`, `TextDecoder`, `atob` and `btoa` — the encoding globals.
//!
//! # What reuse-check found
//!
//! Searched `rts-cranelift` by concern first, as the skill's table says: tags,
//! shapes, layouts, ABI, GC, scheduling. **Nothing there answers text encoding
//! at all**, and that is correct rather than a gap — a buffer's bytes are a
//! runtime table, not something the compiler emits, which is the sentence
//! `entry::bytes_of`'s own doc already records.
//!
//! Searched `rts-core-rwk`'s host surface next (`entry/modules.rs`, read in
//! full). It already exports every codec this module needs: `encode_text`,
//! `decode_bytes`, `canonical_encoding`, `encode_base64`, `decode_base64`,
//! `make_bytes`, `bytes_of`, `write_bytes`. **This file therefore contains no
//! base64 alphabet, no UTF-8 walker, no UTF-16 decoder and no encoding-alias
//! table** — the label folding below goes through `canonical_encoding`, so the
//! alias set is stated once, in `entry::buffer::codec`, and not again here.
//!
//! Searched the workspace for `atob`/`btoa`/`TextEncoder`. Two hits matter:
//!
//! - `rts-node-rwk/src/buffer.rs` has `atob`/`btoa` as members of the
//!   `node:buffer` **namespace**. It is not reachable from here — this crate
//!   depends on `rts-core-rwk` and nothing else — and a global is a different
//!   binding from a namespace member anyway: a program writes `btoa(x)` with no
//!   import line. Both go through `entry::encode_base64`/`decode_base64`, so
//!   there is **one codec with two bindings**, not two codecs. They diverge on
//!   invalid input, and that divergence is named below rather than hidden.
//! - `rts-codegen/src/emit/globals.rs` does not yet list these four names. That
//!   file is not this one's to edit; until it does, a program reaching them is
//!   refused at compile time rather than answered wrongly at run time, which is
//!   the asymmetry that list's own comment states.
//!
//! # Why the class properties are data properties
//!
//! `entry::define_getter` is exported, but a host cannot use it: it takes an
//! interned key NUMBER out of the compiler's registry, and nothing on this
//! surface mints one from a name — and it is ambient, so a class builder holding
//! the context could not call it without a second borrow. So
//! `encoding`/`fatal`/`ignoreBOM` are ordinary data properties, which `node:url`
//! and `node:stream` already record for the same reason.
//!
//! `TextEncoder.prototype.encoding` sits on the **prototype** rather than on
//! each instance, because the specification makes it a prototype accessor:
//! `Object.keys(new TextEncoder())` is `[]` in Node, and an own data property
//! would answer `["encoding"]`. `TextDecoder`'s three vary per instance, so
//! they are own properties and `Object.keys` does diverge there — named below.
//!
//! # Why there is no throw anywhere in this file
//!
//! `rts_core_rwk::entry::throw` ends the program rather than unwinding to a
//! handler; its own module doc says so, and the machine has no exception table
//! yet. Every failure the specification defines as a throw therefore answers
//! `undefined` here — an answer a program can test — rather than a fabricated
//! value or a silent no-op.
//!
//! # Not implemented, by name
//!
//! - **`atob`/`btoa` throwing `InvalidCharacterError`.** Invalid input answers
//!   `undefined`. This differs from `node:buffer`'s `atob`, which answers `""`;
//!   `undefined` is the one a program can distinguish from a successful decode
//!   of an empty string, and `""` is the answer that looks like it worked.
//! - **`new TextDecoder(label)` throwing `RangeError` for a label this engine
//!   cannot decode.** It answers an INERT decoder instead — one carrying no
//!   `encoding`, whose `decode` answers `undefined`. The reference document is
//!   explicit that the wrong move here is "silently accepting an unsupported
//!   label and mis-decoding" (globals.md §4), and the inert instance is
//!   `node:url`'s own precedent for an unrepresentable throw.
//! - **The WHATWG label registry beyond four families.** Supported, with the
//!   label each reports: `utf-8`, `utf-16le` (both exact), `latin1` and
//!   `ascii`. `utf-16be`, `windows-1252`, `shift_jis`, `gbk` and the rest of
//!   the registry are refused — there is no ICU here and no single-byte tables.
//!   `latin1` reports `"latin1"` and not WHATWG's canonical `"windows-1252"`,
//!   because this engine decodes it as ISO-8859-1: bytes `0x80`–`0x9F` become
//!   `U+0080`–`U+009F` and not windows-1252's punctuation. Reporting the
//!   canonical name would be claiming a decode this does not perform. `ascii`
//!   diverges further and is named here for it: the codec MASKS the high bit,
//!   so byte `0xE9` decodes to `i` where every specified behaviour — WHATWG's
//!   and a fatal decoder's alike — produces `U+FFFD` or an error.
//! - **`decoder.fatal`.** Always `false`, whatever `options.fatal` asked for,
//!   because this decoder is never fatal — malformed input becomes `U+FFFD`.
//!   Reporting `true` would describe behaviour the object does not have.
//! - **`decoder.decode(input, { stream: true })`.** Answers `undefined`.
//!   Streaming needs bytes held across calls; without it a chunk split inside a
//!   multi-byte sequence silently gains a `U+FFFD` where the next chunk would
//!   have completed the character — a wrong answer that runs.
//! - **`TextDecoder`'s `encoding`/`fatal`/`ignoreBOM` as inherited accessors.**
//!   They are own data properties, so `Object.keys(new TextDecoder())` answers
//!   three names where Node answers none, and a program may assign to them.
//! - **`TypeError` for a non-`BufferSource` argument.** `decoder.decode({})`
//!   reads no bytes and answers `""`; `encoder.encodeInto(s, {})` answers
//!   `{ read: 0, written: 0 }`.
//! - **`TypeError` for calling either class without `new`.** `TextEncoder()`
//!   builds an instance, the shape every host class in this workspace has while
//!   there is nothing to throw with.
//! - **A string holding a lone surrogate.** `Str::to_rust` answers nothing for
//!   one rather than substituting, so `encoder.encode("\uD800")` answers an
//!   empty `Uint8Array` where the specification's `USVString` conversion would
//!   have produced the three bytes of `U+FFFD`.
//! - **`TextEncoderStream`/`TextDecoderStream`.** `TransformStream`-shaped, and
//!   documented under `node:stream/web` rather than here.

use rts_core_rwk::entry::{self, Context, Provided};

/// Installs `TextEncoder`, `TextDecoder`, `atob` and `btoa` as globals.
pub fn install(context: &mut Context) {
    let encoder = encoder_class(context);
    entry::declare_global(context, "TextEncoder", encoder);
    let decoder = decoder_class(context);
    entry::declare_global(context, "TextDecoder", decoder);
    let atob_global = entry::make_callable(context, atob);
    entry::declare_global(context, "atob", atob_global);
    let btoa_global = entry::make_callable(context, btoa);
    entry::declare_global(context, "btoa", btoa_global);
}

// ---------------------------------------------------------------- TextEncoder

const ENCODER_METHODS: &[(&str, Provided)] =
    &[("encode", encode), ("encodeInto", encode_into)];

/// The `TextEncoder` constructor, with its prototype linked.
fn encoder_class(context: &mut Context) -> u64 {
    let prototype = encoder_prototype(context);
    // `encoding` is written HERE and not in the constructor: `make_prototype`
    // is idempotent by name, so every later reader gets this same object with
    // the property already on it, and a constructor that re-wrote it would
    // intern the string again on every `new`.
    let utf8 = entry::make_string(context, "utf-8");
    entry::put_member(context, prototype, "encoding", utf8);
    let ctor = entry::make_callable(context, new_encoder);
    entry::put_member(context, ctor, "prototype", prototype);
    ctor
}

/// The one `TextEncoder.prototype`, made on the first ask.
fn encoder_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "TextEncoder", ENCODER_METHODS)
}

/// `new TextEncoder()` — no arguments, per the specification.
extern "C" fn new_encoder(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = encoder_prototype(context);
        self_or_new(context, this, prototype)
    })
}

/// `encoder.encode(input?)` — a `Uint8Array` of the UTF-8 bytes.
extern "C" fn encode(_e: u64, _this: u64, input: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = text_argument(input).unwrap_or_default();
    let bytes = entry::encode_text(&text, "utf8").unwrap_or_default();
    entry::with_runtime(|context| entry::make_bytes(context, &bytes))
}

/// `encoder.encodeInto(source, destination)` — `{ read, written }`.
///
/// # Why the destination is measured before anything is written
///
/// `entry::write_bytes` truncates at the window's end, and a truncation at an
/// arbitrary byte would leave HALF a multi-byte sequence in the caller's array
/// — which the specification forbids and which no later call can repair. So the
/// prefix that fits is computed first, in whole characters, and only that is
/// written.
///
/// Measuring costs a copy of the destination, which `entry::bytes_of` names in
/// its own doc. The alternative was the `byteLength` property typed arrays carry
/// — rejected because it is an ordinary writable data property here, so a
/// program that assigned to it would be deciding how many bytes this writes.
extern "C" fn encode_into(
    _e: u64,
    _this: u64,
    source: u64,
    destination: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let text = text_argument(source).unwrap_or_default();
    entry::with_runtime(|context| {
        let room = entry::bytes_of(context, destination).map_or(0, |bytes| bytes.len());
        let (read, fitting) = fitting_prefix(&text, room);
        let written = entry::write_bytes(context, destination, 0, fitting);
        let result = entry::make_object(context);
        let read_value = entry::make_number(read as f64);
        entry::put_member(context, result, "read", read_value);
        let written_value = entry::make_number(written as f64);
        entry::put_member(context, result, "written", written_value);
        result
    })
}

/// The longest whole-character prefix of `text` whose UTF-8 form fits in `room`
/// bytes, with the number of UTF-16 code units it spans.
///
/// UTF-16 units and not characters, because `read` is what a JavaScript caller
/// slices the remaining source with and a JavaScript string is indexed in UTF-16
/// units. `char::len_utf8`/`len_utf16` answer both widths, which is why this
/// walks characters rather than re-deriving either encoding.
fn fitting_prefix(text: &str, room: usize) -> (usize, &[u8]) {
    let mut units = 0usize;
    let mut end = 0usize;
    for character in text.chars() {
        let width = character.len_utf8();
        if end + width > room {
            break;
        }
        end += width;
        units += character.len_utf16();
    }
    (units, &text.as_bytes()[..end])
}

// ---------------------------------------------------------------- TextDecoder

const DECODER_METHODS: &[(&str, Provided)] = &[("decode", decode)];

/// The `TextDecoder` constructor, with its prototype linked.
fn decoder_class(context: &mut Context) -> u64 {
    let prototype = decoder_prototype(context);
    let ctor = entry::make_callable(context, new_decoder);
    entry::put_member(context, ctor, "prototype", prototype);
    ctor
}

/// The one `TextDecoder.prototype`, made on the first ask.
fn decoder_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "TextDecoder", DECODER_METHODS)
}

/// `new TextDecoder(label?, options?)`.
///
/// An unsupported label produces an instance with no `encoding` at all — see
/// the module doc for why that is the honest shape of a refusal here.
extern "C" fn new_decoder(_e: u64, this: u64, label: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let asked = text_argument(label).unwrap_or_else(|| "utf-8".to_owned());
    let reported = supported_label(&asked);
    let requested_bom = entry::with_runtime(|context| option_value(context, options, "ignoreBOM"));
    // Decoded OUTSIDE the borrow above: `to_boolean` is an ambient entry point
    // that takes its own borrow, and a second one inside an `extern "C"` frame
    // is a panic that cannot unwind — it aborts the process.
    let ignore_bom = entry::to_boolean(requested_bom);
    entry::with_runtime(|context| {
        let prototype = decoder_prototype(context);
        let instance = self_or_new(context, this, prototype);
        if let Some(reported) = reported {
            let encoding = entry::make_string(context, reported);
            entry::put_member(context, instance, "encoding", encoding);
            // Always false: this decoder substitutes `U+FFFD` and never throws,
            // so `true` would describe something it does not do.
            entry::put_member(context, instance, "fatal", entry::boolean_value(false));
            let bom = entry::boolean_value(ignore_bom);
            entry::put_member(context, instance, "ignoreBOM", bom);
        }
        instance
    })
}

/// `decoder.decode(input?, options?)`.
extern "C" fn decode(_e: u64, this: u64, input: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let streaming = entry::with_runtime(|context| option_value(context, options, "stream"));
    if entry::to_boolean(streaming) {
        return entry::undefined_value();
    }
    entry::with_runtime(|context| {
        let reported = entry::get_member(context, this, "encoding");
        let codec = entry::string_in(context, reported)
            .and_then(|label| supported_label(&label))
            .and_then(|label| entry::canonical_encoding(label));
        let Some(codec) = codec else {
            return entry::undefined_in(context);
        };
        let bytes = entry::bytes_of(context, input).unwrap_or_default();
        let text = entry::decode_bytes(&bytes, codec);
        let ignore = entry::get_member(context, this, "ignoreBOM");
        // A bit comparison and not `to_boolean`, which would be a nested borrow:
        // this property is one this module wrote as a real boolean, so the bits
        // are exact rather than a coercion standing in for one.
        let stripped = match ignore == entry::boolean_value(true) {
            true => None,
            // Only `U+FEFF` is stripped, so the single-byte codecs need no case
            // of their own: `EF BB BF` under `latin1` decodes to three ordinary
            // characters and never matches.
            false => text.strip_prefix('\u{FEFF}').map(|rest| rest.to_owned()),
        };
        entry::make_string(context, &stripped.unwrap_or(text))
    })
}

/// The label a decoder reports for an asked-for one, or nothing when this
/// engine cannot decode it.
///
/// The alias folding is `canonical_encoding`'s — `UTF-8`, `utf8`, `ucs-2` and
/// the rest are its table's business, stated once. What this adds is the
/// membership question that table does not answer: `base64`, `base64url` and
/// `hex` are byte-to-text codecs rather than character encodings, and a
/// `TextDecoder` accepting them would decode bytes into their own transcription.
fn supported_label(label: &str) -> Option<&'static str> {
    match entry::canonical_encoding(label)? {
        "utf8" => Some("utf-8"),
        "utf16le" => Some("utf-16le"),
        "latin1" => Some("latin1"),
        "ascii" => Some("ascii"),
        _ => None,
    }
}

// -------------------------------------------------------------- atob and btoa

/// `atob(data)` — base64 to a binary string, one code unit per byte.
///
/// `undefined` for input that is not valid forgiving-base64, where Node throws
/// `InvalidCharacterError`.
extern "C" fn atob(_e: u64, _this: u64, data: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = text_argument(data) else {
        return entry::undefined_value();
    };
    if !is_forgiving_base64(&text) {
        return entry::undefined_value();
    }
    let bytes = entry::decode_base64(&text);
    let binary = entry::decode_bytes(&bytes, "latin1");
    entry::with_runtime(|context| entry::make_string(context, &binary))
}

/// `btoa(data)` — a binary string to base64.
///
/// `undefined` for a code point above `U+00FF`, where Node throws
/// `InvalidCharacterError`. Checked BEFORE encoding, because `latin1` encoding
/// truncates such a character to its low byte — which would answer base64 of
/// something the caller never wrote.
extern "C" fn btoa(_e: u64, _this: u64, data: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(text) = text_argument(data) else {
        return entry::undefined_value();
    };
    if text.chars().any(|character| character as u32 > 0xFF) {
        return entry::undefined_value();
    }
    let Some(bytes) = entry::encode_text(&text, "latin1") else {
        return entry::undefined_value();
    };
    let encoded = entry::encode_base64(&bytes, true);
    entry::with_runtime(|context| entry::make_string(context, &encoded))
}

/// Whether text is valid input to WHATWG "forgiving-base64 decode".
///
/// # Why validity is asked here when decoding is not
///
/// `entry::decode_base64` is deliberately permissive — its own doc says it
/// ignores whatever it does not recognise — because that is what
/// `Buffer.from(s, "base64")` wants. `atob` wants the opposite, and the two
/// cannot be one function. So this asks the question the decoder does not, and
/// decodes nothing itself: the alphabet appears here as a membership test over
/// characters, never as a table of values, and the bytes still come from the one
/// decoder in the workspace.
///
/// The algorithm is the specification's, in its order: strip ASCII whitespace,
/// drop at most two trailing `=` when the length is a multiple of four, refuse a
/// remainder of one, then require every remaining character to be in the
/// standard alphabet.
fn is_forgiving_base64(text: &str) -> bool {
    let mut stripped: String = text
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect();
    let padding = match stripped.len() % 4 == 0 {
        true => (stripped.len() - stripped.trim_end_matches('=').len()).min(2),
        false => 0,
    };
    let kept = stripped.len() - padding;
    stripped.truncate(kept);
    if stripped.len() % 4 == 1 {
        return false;
    }
    stripped
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '+' || character == '/')
}

// ----------------------------------------------------------------- the shared

/// An argument as text, `None` for an absent one.
///
/// `text_of` is `ToString`, which is what a `DOMString` parameter takes — so
/// `btoa(123)` encodes `"123"`, as it does in Node. It answers nothing for an
/// object, whose `toString` is user code no entry point may call, and for a
/// string holding a lone surrogate.
///
/// Ambient, so every caller of this is outside a borrow.
fn text_argument(value: u64) -> Option<String> {
    let absent = entry::undefined_value();
    match value == absent {
        true => None,
        false => entry::text_of(value),
    }
}

/// One member of an options bag, `undefined` when there is no bag.
///
/// Context-taking throughout: an options argument is read while the caller holds
/// the borrow, where the ambient `get_indexed` would be a nested one.
fn option_value(context: &mut Context, options: u64, name: &str) -> u64 {
    match entry::is_object(context, options) {
        true => entry::get_member(context, options, name),
        false => entry::undefined_in(context),
    }
}

/// `this` when `new` already made one, else a fresh instance.
///
/// Four lines copied rather than reached for: `rts-node-rwk`'s copies are
/// `pub(super)` inside their own modules and this crate does not depend on that
/// crate at all, so widening someone's visibility across a crate boundary would
/// have cost more than the four lines.
fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}
