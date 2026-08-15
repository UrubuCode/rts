//! `TextDecoder` — with bytes held between calls, and a `fatal` mode that
//! really throws.
//!
//! # What this file decodes, and what it does NOT
//!
//! It contains no encoding table, no UTF-8 walker and no UTF-16 decoder. Every
//! byte that becomes text goes through `entry::decode_bytes`, which is the one
//! codec in this workspace and the one `Buffer` uses. What this file adds is the
//! question that codec deliberately does not answer: **where does the input
//! stop being decodable**, so that a chunk boundary landing inside a character
//! can be held over to the next call rather than turned into `U+FFFD`.
//!
//! `std::str::from_utf8` is what answers it, and it is used as a boundary finder
//! rather than as a decoder — `Utf8Error::valid_up_to` and `error_len` are the
//! two numbers the codec has no way to report. Reimplementing UTF-8 to get them
//! is what this arrangement refuses.
//!
//! # Why the pending bytes live beside the object rather than on it
//!
//! The same reason `Blob`'s do, one crate over: a property is assignable, and a
//! program that wrote `decoder.__pending = 0` would corrupt a decode rather than
//! be ignored. So the tail is a `Vec<u8>` in [`TABLE`] and the instance carries
//! only the number that finds it. It inherits the same stated cost: nothing here
//! drives a `FinalizationRegistry`, so a table entry outlives the decoder that
//! named it.
//!
//! # Why `fatal` throws instead of answering something
//!
//! Because the specification says so and because a native here CAN now — rule 8
//! of `crates/rts-core/README.md` is what made that safe, and `entry::
//! throw_type_error` builds the program's own `TypeError`, so
//! `e instanceof TypeError` holds. A `fatal` decoder that substituted `U+FFFD`
//! anyway would be a surface that cannot do what its name means, which is the
//! rule that already cost this project a whole namespace.
//!
//! # Not implemented, by name
//!
//! - **`fatal` for `latin1`.** Every byte is a valid ISO-8859-1 character, so
//!   there is nothing for it to reject — that is the encoding, not a gap.
//! - **`stream: true` holding state for `latin1`/`ascii`.** Neither has a
//!   multi-byte sequence to split, so a chunk boundary can never fall inside a
//!   character and there is nothing to hold.
//! - **A decoder shared between two threads.** [`TABLE`] is process-global and
//!   keyed by a number, which is what makes that safe; what is not offered is a
//!   decoder *object* crossing, because no value crosses in this engine.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core::entry::{self, Context, Provided};

/// What one decoder carries between calls.
///
/// `started` is not bookkeeping for its own sake: a BOM is stripped from the
/// beginning of a STREAM, and without this a second chunk that happens to begin
/// with `U+FEFF` would lose a character the specification keeps.
#[derive(Default)]
struct State {
    /// Bytes a streaming call could not finish, waiting for the next one.
    pending: Vec<u8>,
    /// Whether anything has been decoded since the decoder was last reset.
    started: bool,
}

static TABLE: Mutex<Option<HashMap<u64, State>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, State>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[("decode", decode)];

/// The `TextDecoder` constructor, with its prototype linked.
pub(super) fn class(context: &mut Context) -> u64 {
    let prototype = prototype(context);
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::put_member(context, prototype, "constructor", ctor);
    // `name` as a data property: a native callable carries none in this engine,
    // so `x.constructor.name` reads `undefined` without it.
    let held = entry::make_string(context, "TextDecoder");
    entry::put_member(context, ctor, "name", held);
    ctor
}

/// The one `TextDecoder.prototype`, made on the first ask.
fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "TextDecoder", METHODS)
}

/// `new TextDecoder(label?, options?)`.
///
/// An unsupported label produces an instance with no `encoding` at all — see
/// the module doc for why that is the honest shape of a refusal here.
extern "C" fn construct(_e: u64, this: u64, label: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let asked = super::text_argument(label).unwrap_or_else(|| "utf-8".to_owned());
    let reported = supported_label(&asked);
    let (requested_bom, requested_fatal) = entry::with_runtime(|context| {
        (
            super::option_value(context, options, "ignoreBOM"),
            super::option_value(context, options, "fatal"),
        )
    });
    // Decoded OUTSIDE the borrow above: `to_boolean` is an ambient entry point
    // that takes its own borrow, and a second one inside an `extern "C"` frame
    // is a panic that cannot unwind — it aborts the process.
    let ignore_bom = entry::to_boolean(requested_bom);
    let fatal = entry::to_boolean(requested_fatal);
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    entry::with_runtime(|context| {
        let prototype = prototype(context);
        let instance = super::self_or_new(context, this, prototype);
        if let Some(reported) = reported {
            let encoding = entry::make_string(context, reported);
            entry::put_member(context, instance, "encoding", encoding);
            // What was ASKED for, because the decoder now honours it: a
            // `fatal` decoder here really does throw, so reporting `false`
            // would be the lie this property used to tell in the other
            // direction.
            entry::put_member(context, instance, "fatal", entry::boolean_value(fatal));
            let bom = entry::boolean_value(ignore_bom);
            entry::put_member(context, instance, "ignoreBOM", bom);
            let held = entry::make_number(id as f64);
            entry::put_member(context, instance, "__decoderId", held);
        }
        instance
    })
}

/// What a decoder was told to be, read back off the instance.
struct Settings {
    codec: &'static str,
    fatal: bool,
    ignore_bom: bool,
    id: u64,
}

/// Reads all four in ONE borrow — `get_member` interns a name per call, and a
/// decode that took four borrows to learn how to decode would pay for it per
/// chunk rather than per decoder.
fn settings_of(context: &mut Context, this: u64) -> Option<Settings> {
    let reported = entry::get_member(context, this, "encoding");
    let codec = entry::string_in(context, reported)
        .and_then(|label| supported_label(&label))
        .and_then(entry::canonical_encoding)?;
    let truth = entry::boolean_value(true);
    Some(Settings {
        codec,
        // A bit comparison and not `to_boolean`, which would be a nested
        // borrow: both properties are ones this module wrote as real booleans,
        // so the bits are exact rather than a coercion standing in for one.
        fatal: entry::get_member(context, this, "fatal") == truth,
        ignore_bom: entry::get_member(context, this, "ignoreBOM") == truth,
        id: entry::number_of(entry::get_member(context, this, "__decoderId"))? as u64,
    })
}

/// `decoder.decode(input?, options?)`.
///
/// The order is forced by the borrow rule: everything is collected under one
/// borrow, the decision about what is decodable is made with none held, and the
/// throw — which runs the program's own `TypeError` constructor — happens last,
/// outside everything.
extern "C" fn decode(_e: u64, this: u64, input: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let collected = entry::with_runtime(|context| {
        let settings = settings_of(context, this)?;
        let bytes = entry::bytes_of(context, input).unwrap_or_default();
        Some((settings, bytes, super::option_value(context, options, "stream")))
    });
    let Some((settings, bytes, requested_stream)) = collected else {
        return entry::undefined_value();
    };
    let streaming = entry::to_boolean(requested_stream);

    let mut state = with_table(|table| table.remove(&settings.id).unwrap_or_default());
    state.pending.extend_from_slice(&bytes);
    let split = split_decodable(&state.pending, settings.codec, streaming);
    if settings.fatal && split.malformed {
        // The state is dropped rather than kept: a fatal decoder that threw is
        // done, and holding bytes for a call that would throw again on the same
        // input is a decoder that can never recover.
        return fail(settings.codec);
    }
    let text = entry::decode_bytes(&state.pending[..split.decodable], settings.codec);
    let at_start = !state.started;
    // A non-streaming call ends the stream, and the specification resets the
    // decoder there — which is why the next `decode` strips a BOM again.
    let kept = match streaming {
        true => State { pending: split.held, started: state.started || split.decodable > 0 },
        false => State::default(),
    };
    with_table(|table| table.insert(settings.id, kept));
    // Only `U+FEFF` is stripped, and only at the START of a stream: a later
    // chunk beginning with one carries a character the program asked for. The
    // single-byte codecs need no case of their own — `EF BB BF` under `latin1`
    // decodes to three ordinary characters and never matches.
    let text = match settings.ignore_bom || !at_start {
        true => text,
        false => text.strip_prefix('\u{FEFF}').map(str::to_owned).unwrap_or(text),
    };
    entry::with_runtime(|context| entry::make_string(context, &text))
}

/// Raises the `TypeError` a `fatal` decoder owes, and answers `undefined`.
///
/// The value is never observed — the compiled call site checks for a pending
/// throw immediately after the call and re-raises — but a native has to answer
/// something, and `undefined` is the one every other raising path here uses.
fn fail(codec: &str) -> u64 {
    entry::throw_type_error(&format!(
        "The encoded data was not valid for encoding {codec}"
    ));
    entry::undefined_value()
}

/// How much of a buffer can be turned into text right now.
struct Split {
    /// Bytes to hand to the codec.
    decodable: usize,
    /// Bytes to keep for the next call.
    held: Vec<u8>,
    /// Whether what could not be decoded is broken rather than merely
    /// unfinished — which is the difference between a `fatal` throw and a
    /// legitimate chunk boundary.
    malformed: bool,
}

/// Where the decodable prefix ends, per codec.
///
/// `streaming` is what decides whether an unfinished tail is held or is an
/// error: the final call of a stream has nothing more coming, so a partial
/// sequence there is malformed input rather than a boundary.
fn split_decodable(bytes: &[u8], codec: &str, streaming: bool) -> Split {
    match codec {
        "utf8" => split_utf8(bytes, streaming),
        "utf16le" => split_utf16le(bytes, streaming),
        // One byte, one character: nothing can be split and nothing is
        // invalid. `ascii` is included here rather than given a high-bit check
        // because this engine's `ascii` codec MASKS the high bit — a divergence
        // the parent module states — so there is no byte it rejects.
        _ => Split { decodable: bytes.len(), held: Vec::new(), malformed: false },
    }
}

/// UTF-8, using `std::str::from_utf8` as the boundary finder the codec is not.
///
/// The two error shapes mean different things and this is the whole reason the
/// function exists: `error_len() == None` says the buffer ENDS inside a
/// sequence that could still be completed, which is exactly a chunk boundary;
/// `Some(n)` says the bytes at that position can never be part of a valid
/// sequence, which is malformed input whether or not more is coming.
fn split_utf8(bytes: &[u8], streaming: bool) -> Split {
    match std::str::from_utf8(bytes) {
        Ok(_) => Split { decodable: bytes.len(), held: Vec::new(), malformed: false },
        Err(error) => match (error.error_len(), streaming) {
            (None, true) => Split {
                decodable: error.valid_up_to(),
                held: bytes[error.valid_up_to()..].to_vec(),
                malformed: false,
            },
            // A truncated sequence at the end of the LAST chunk, or a genuinely
            // invalid byte anywhere: both are malformed, and the whole buffer
            // still goes to the codec so a non-fatal decoder gets its `U+FFFD`
            // in the right place.
            _ => Split { decodable: bytes.len(), held: Vec::new(), malformed: true },
        },
    }
}

/// UTF-16LE: an odd trailing byte is half a code unit, and a trailing high
/// surrogate is half a character.
///
/// The second case is the one that is easy to miss and impossible to repair
/// later: a chunk ending on `D8 3D` decodes to `U+FFFD` on its own, and the
/// `DE 42` that would have completed the emoji decodes to a second `U+FFFD` on
/// the next call. Holding two bytes back is what makes the pair survive.
fn split_utf16le(bytes: &[u8], streaming: bool) -> Split {
    let mut whole = bytes.len() - bytes.len() % 2;
    let trailing_high = whole >= 2 && {
        let unit = u16::from_le_bytes([bytes[whole - 2], bytes[whole - 1]]);
        (0xD800..=0xDBFF).contains(&unit)
    };
    if streaming && trailing_high {
        whole -= 2;
    }
    match streaming {
        true => Split { decodable: whole, held: bytes[whole..].to_vec(), malformed: false },
        // On the final call an odd byte or an unpaired surrogate is malformed —
        // there is nothing left to complete it with.
        false => Split {
            decodable: bytes.len(),
            held: Vec::new(),
            malformed: bytes.len() % 2 != 0 || trailing_high,
        },
    }
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
