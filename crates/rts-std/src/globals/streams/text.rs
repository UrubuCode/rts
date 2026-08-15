//! `TextEncoderStream` and `TextDecoderStream`.
//!
//! # Neither of these decodes anything
//!
//! `TextDecoderStream`'s whole difficulty is that a chunk boundary may fall
//! inside a multi-byte character, and `globals/text/decoder.rs` already answers
//! it — `decode(chunk, { stream: true })` holds the unfinished tail over to the
//! next call, and `std::str::from_utf8`'s `valid_up_to`/`error_len` are what
//! decide where the split is. So this file holds a real `TextDecoder` instance
//! on its sink and calls that method. There is no second UTF-8 boundary walker
//! here, and a `fatal` or `utf-16le` stream inherits that one's behaviour for
//! free rather than by a second implementation agreeing with it.
//!
//! The instance is reached through the `TextDecoder` GLOBAL rather than through
//! `globals::text`'s own constructor, because that module's `decoder` submodule
//! is private — the same route `globals/fetch/` takes to `node:buffer`'s
//! `Blob`, and for the same reason: one class, not two that fail an
//! `instanceof`.
//!
//! # Not implemented, by name
//!
//! - **A surrogate pair split across two `TextEncoderStream` chunks.** The
//!   specification holds a lone leading surrogate back for the next chunk;
//!   here each chunk is encoded on its own, and `entry::text_of` answers
//!   nothing for a string holding a lone surrogate — so such a chunk
//!   contributes no bytes rather than the wrong ones. The same limit
//!   `globals/text/mod.rs` states for `encoder.encode`.
//! - **`new TextDecoderStream(label)` throwing `RangeError`.** An unsupported
//!   label produces an inert stream whose readable closes empty, which is
//!   `TextDecoder`'s own decision one file over.

use rts_core::entry::{self, Context, Provided};

use super::{field, threw, transform};

/// The `TextDecoder` a decoding stream holds, and the options object it passes
/// it — built once per stream rather than per chunk.
const DECODER: &str = "__decoder";
const STREAMING: &str = "__streaming";

const ENCODER_METHODS: &[(&str, Provided)] = &[];
const DECODER_METHODS: &[(&str, Provided)] = &[];

/// The `TextEncoderStream` and `TextDecoderStream` constructors.
pub(super) fn classes(context: &mut Context) -> (u64, u64) {
    let prototype = encoder_prototype(context);
    // On the PROTOTYPE, the way `TextEncoder.prototype.encoding` is: the
    // specification makes it a prototype accessor, so `Object.keys(stream)`
    // must not answer it.
    let utf8 = entry::make_string(context, "utf-8");
    entry::put_member(context, prototype, "encoding", utf8);
    let encoder = super::class_of(context, "TextEncoderStream", prototype, construct_encoder);
    let prototype = decoder_prototype(context);
    let decoder = super::class_of(context, "TextDecoderStream", prototype, construct_decoder);
    (encoder, decoder)
}

fn encoder_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "TextEncoderStream", ENCODER_METHODS)
}

fn decoder_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "TextDecoderStream", DECODER_METHODS)
}

// -------------------------------------------------------- TextEncoderStream

/// `new TextEncoderStream()` — no arguments, per the specification.
extern "C" fn construct_encoder(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = encoder_prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        transform::pair(context, instance, encode_write, encode_close);
        instance
    })
}

/// One chunk of text to its UTF-8 bytes. `this` is the sink.
extern "C" fn encode_write(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let text = entry::text_of(chunk).unwrap_or_default();
    let bytes = entry::encode_text(&text, "utf8").unwrap_or_default();
    if !bytes.is_empty() {
        let held = entry::with_runtime(|context| entry::make_bytes(context, &bytes));
        super::readable::enqueue(field(this, transform::READABLE), held);
    }
    absent
}

extern "C" fn encode_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::readable::close(field(this, transform::READABLE));
    entry::undefined_value()
}

// -------------------------------------------------------- TextDecoderStream

/// `new TextDecoderStream(label?, options?)`.
extern "C" fn construct_decoder(_e: u64, this: u64, label: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let class = super::global("TextDecoder");
    if !entry::with_runtime(|context| entry::is_callable_in(context, class)) {
        return absent;
    }
    let decoder = entry::construct(class, label, options, absent, absent);
    if threw() {
        return absent;
    }
    entry::with_runtime(|context| {
        let prototype = decoder_prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        let sink = transform::pair(context, instance, decode_write, decode_close);
        entry::put_member(context, sink, DECODER, decoder);
        // `{ stream: true }`, made once: a decoding stream reads it per chunk.
        let streaming = entry::make_object(context);
        entry::put_member(context, streaming, "stream", entry::boolean_value(true));
        entry::put_member(context, sink, STREAMING, streaming);
        // The three facts a program reads off the stream are the decoder's
        // own, copied rather than re-derived — an unsupported label leaves the
        // decoder carrying none of them, and this stream carries none either.
        for name in ["encoding", "fatal", "ignoreBOM"] {
            let held = entry::get_member(context, decoder, name);
            if held != entry::undefined_in(context) {
                entry::put_member(context, instance, name, held);
            }
        }
        instance
    })
}

/// One chunk of bytes through the held `TextDecoder`. `this` is the sink.
///
/// Nothing is enqueued for an empty answer, which is the specification's rule
/// and not an optimisation: a chunk that ends mid-character decodes to `""`
/// with its bytes held over, and enqueuing that would put an empty string into
/// a program's output between two real ones.
extern "C" fn decode_write(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let decoder = field(this, DECODER);
    let method = field(decoder, "decode");
    let options = field(this, STREAMING);
    let text = entry::call(method, decoder, chunk, options, absent, absent);
    if threw() {
        return absent;
    }
    emit(this, text);
    absent
}

/// The stream ends: one last non-streaming `decode()`, which is what flushes a
/// held-over partial sequence, then the readable half closes.
extern "C" fn decode_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let decoder = field(this, DECODER);
    let method = field(decoder, "decode");
    let text = entry::call(method, decoder, absent, absent, absent, absent);
    if threw() {
        return absent;
    }
    emit(this, text);
    super::readable::close(field(this, transform::READABLE));
    absent
}

/// Enqueues a decoded chunk, unless there is nothing in it.
fn emit(sink: u64, text: u64) {
    let held = entry::with_runtime(|context| entry::string_in(context, text));
    if held.is_some_and(|held| !held.is_empty()) {
        super::readable::enqueue(field(sink, transform::READABLE), text);
    }
}
