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
//! Searched `rts-core`'s host surface next (`entry/modules.rs`, read in
//! full). It already exports every codec this module needs: `encode_text`,
//! `decode_bytes`, `canonical_encoding`, `encode_base64`, `decode_base64`,
//! `make_bytes`, `bytes_of`, `write_bytes`. **This file therefore contains no
//! base64 alphabet, no UTF-8 walker, no UTF-16 decoder and no encoding-alias
//! table** — the label folding below goes through `canonical_encoding`, so the
//! alias set is stated once, in `entry::buffer::codec`, and not again here.
//!
//! Searched the workspace for `atob`/`btoa`/`TextEncoder`. Two hits matter:
//!
//! - `rts-node/src/buffer.rs` has `atob`/`btoa` as members of the
//!   `node:buffer` **namespace**. It is not reachable from here — this crate
//!   depends on `rts-core` and nothing else — and a global is a different
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
//! This paragraph used to say a host could not install an accessor at all,
//! because `entry::define_getter` takes an interned key NUMBER and nothing minted
//! one from a name. `entry::member_key` does — `node:url` installs eleven real
//! accessor pairs through it. So the reason here is narrower now and it is a
//! choice: `encoding`/`fatal`/`ignoreBOM` never change after construction, so an
//! accessor would call a function to read a constant. What a program can still
//! do that Node forbids is ASSIGN to them, which is named below.
//!
//! # Where `TextDecoder` went
//!
//! Into [`decoder`], because holding bytes between calls made this file pass the
//! 500-line ceiling. The split is by class rather than by "the big function",
//! which is what keeps the streaming state, the codec choice and the fatal
//! decision in one place instead of three.
//!
//! `TextEncoder.prototype.encoding` sits on the **prototype** rather than on
//! each instance, because the specification makes it a prototype accessor:
//! `Object.keys(new TextEncoder())` is `[]` in Node, and an own data property
//! would answer `["encoding"]`. `TextDecoder`'s three vary per instance, so
//! they are own properties and `Object.keys` does diverge there — named below.
//!
//! # Why most failures still do not throw in this file
//!
//! `rts_core::entry::throw` can now preserve a throw for compiled code to catch,
//! but older decoder paths still answer `undefined` where their specification
//! raises. The distinction is kept per operation rather than turning every
//! refusal into a process-visible throw without a measured contract.
//!
//! # Not implemented, by name
//!
//! - **`new TextDecoder(label)` throwing `RangeError` for a label this engine
//!   cannot decode.** It answers an INERT decoder instead — one carrying no
//!   `encoding`, whose `decode` answers `undefined`. The reference document is
//!   explicit that the wrong move here is "silently accepting an unsupported
//!   label and mis-decoding" (globals.md §4), and the inert instance is
//!   `node:url`'s own precedent for an unrepresentable throw. A native can raise
//!   now — [`decoder`]'s `fatal` mode does — so this is a decision left standing
//!   rather than a wall: the label set is small, and turning every unsupported
//!   one into a process-visible throw is a change nothing has measured.
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

mod decoder;

use rts_core::entry::{self, Context, Provided};

/// Installs `TextEncoder`, `TextDecoder`, `atob` and `btoa` as globals.
pub fn install(context: &mut Context) {
    let encoder = encoder_class(context);
    entry::declare_global(context, "TextEncoder", encoder);
    let decoder = decoder::class(context);
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
    // A primitive string already owns its code units in the runtime. Avoid
    // materialising a Rust `String` and then copying it again into the byte
    // vector; non-string inputs retain the DOMString coercion path.
    let bytes = entry::utf8_bytes_if_string(input).unwrap_or_else(|| {
        let text = text_argument(input).unwrap_or_default();
        entry::encode_text(&text, "utf8").unwrap_or_default()
    });
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

// -------------------------------------------------------------- atob and btoa

/// `atob(data)` — base64 to a binary string, one code unit per byte.
///
/// Invalid input raises `InvalidCharacterError`, as required by the WHATWG
/// forgiving-base64 API.
extern "C" fn atob(_e: u64, _this: u64, data: u64, b: u64, c: u64, d: u64) -> u64 {
    let Some(text) = base64_argument([data, b, c, d]) else {
        return entry::undefined_value();
    };
    if !is_forgiving_base64(&text) {
        return invalid_character();
    }
    let bytes = entry::decode_base64(&text);
    let binary = entry::decode_bytes(&bytes, "latin1");
    entry::with_runtime(|context| entry::make_string(context, &binary))
}

/// `btoa(data)` — a binary string to base64.
///
/// A code point above `U+00FF` raises `InvalidCharacterError` BEFORE encoding,
/// because `latin1` would otherwise truncate it to a byte the caller did not
/// provide.
extern "C" fn btoa(_e: u64, _this: u64, data: u64, b: u64, c: u64, d: u64) -> u64 {
    let Some(text) = base64_argument([data, b, c, d]) else {
        return entry::undefined_value();
    };
    if text.chars().any(|character| character as u32 > 0xFF) {
        return invalid_character();
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

/// A base64 argument as text, `None` after raising or preserving its error.
///
/// `string_for_host` runs the existing ToPrimitive protocol, including
/// `Symbol.toPrimitive`, wrappers and user-defined `toString`/`valueOf`. An
/// absent argument and a Symbol are TypeErrors; a callback that throws is left
/// untouched so the compiled caller can propagate its original value.
fn base64_argument(slots: [u64; 4]) -> Option<String> {
    let given = entry::with_runtime(|context| entry::arguments_at(context, 0, slots));
    let Some(value) = given.first().copied() else {
        entry::throw_type_error("The first argument must be specified");
        return None;
    };
    match entry::string_for_host(value) {
        Err(()) => None,
        Ok(Some(text)) => Some(text),
        Ok(None) => {
            entry::throw_type_error("Cannot convert a Symbol value to a string");
            None
        }
    }
}

/// An argument as already-materialised text for the older decoder paths.
fn text_argument(value: u64) -> Option<String> {
    let absent = entry::undefined_value();
    match value == absent {
        true => None,
        false => entry::text_of(value),
    }
}

/// Raises the DOM error used by WHATWG base64 for malformed input.
fn invalid_character() -> u64 {
    let error = super::dom_exception::make("Invalid character", "InvalidCharacterError");
    entry::throw_value(error);
    entry::undefined_value()
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
/// Four lines copied rather than reached for: `rts-node`'s copies are
/// `pub(super)` inside their own modules and this crate does not depend on that
/// crate at all, so widening someone's visibility across a crate boundary would
/// have cost more than the four lines.
fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}
