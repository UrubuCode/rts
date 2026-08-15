//! `CompressionStream` and `DecompressionStream` — real DEFLATE, over `flate2`.
//!
//! # What this repeats from `node:zlib`, and what it does not
//!
//! `rts-node`'s `zlib/codec.rs` reached the same conclusion this file starts
//! from, and its reasoning is quoted rather than re-argued: the `write::`
//! adapters over a drained `Vec` ARE `flate2`'s own output pump, so driving
//! `Compress`/`Decompress` by hand here would be a second copy of it. That is
//! ~40 lines of the same shape in two crates, and it is not reachable: that
//! module is `pub(super)` inside a crate this one does not depend on, and
//! `rts-std` depending on `rts-node` would invert the layering `globals/mod.rs`
//! states. What is NOT repeated is the part that carries the decisions — Node's
//! nine codecs, its option bag, `maxOutputLength`, `finishFlush` — because the
//! WHATWG surface has none of them: three format names and no options at all.
//!
//! # The compression level is zlib's default, and that is observable
//!
//! `Compression::default()` is level 6, which is what `Z_DEFAULT_COMPRESSION`
//! resolves to — so `new CompressionStream("gzip")` here produces the same byte
//! count as Node's and Bun's for the same input. That is a fact a program can
//! read (`chunk.value.length`), which is why it is pinned to a named default
//! rather than left to whatever `flate2` picks.
//!
//! # Why the codec state is a Rust table when nothing else here is
//!
//! Because a half-finished DEFLATE stream is bytes and a window, not a
//! JavaScript value — the collector has nothing to see. That is the side of the
//! split `globals/fetch/mod.rs` draws that `Blob` and `TextDecoder` are already
//! on, and it inherits their stated cost: an entry is removed when the stream
//! closes, so a stream that is never closed leaves one behind.
//!
//! # Not implemented, by name
//!
//! - **A chunk that is not a `BufferSource`.** Node throws `TypeError`; here it
//!   contributes no bytes. The chunk is read through `entry::bytes_of`, which
//!   answers nothing for a string.
//! - **`format: "deflate-raw"` on `DecompressionStream` accepting a zlib
//!   header.** Each format decodes exactly its own framing, which is what the
//!   name means.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::Compression;
use flate2::write::{DeflateDecoder, DeflateEncoder, GzDecoder, GzEncoder, ZlibDecoder, ZlibEncoder};
use rts_core::entry::{self, Context, Provided};

use super::{field, transform};

/// The [`TABLE`] key a sink carries.
const CODEC: &str = "__codec";

const METHODS: &[(&str, Provided)] = &[];

/// One stream's half-finished DEFLATE state.
///
/// Six variants and not a `Box<dyn Write>`: the terminal step differs per
/// adapter (`finish` consumes `self` and answers the sink), which is not
/// something a trait object can express without the extra trait `node:zlib`
/// writes for exactly that reason.
enum Coder {
    Gzip(GzEncoder<Vec<u8>>),
    Zlib(ZlibEncoder<Vec<u8>>),
    Raw(DeflateEncoder<Vec<u8>>),
    Gunzip(GzDecoder<Vec<u8>>),
    Inflate(ZlibDecoder<Vec<u8>>),
    RawInflate(DeflateDecoder<Vec<u8>>),
}

/// One arm per variant, written once.
macro_rules! on_coder {
    ($coder:expr, $held:ident => $body:expr) => {
        match $coder {
            Coder::Gzip($held) => $body,
            Coder::Zlib($held) => $body,
            Coder::Raw($held) => $body,
            Coder::Gunzip($held) => $body,
            Coder::Inflate($held) => $body,
            Coder::RawInflate($held) => $body,
        }
    };
}

impl Coder {
    /// Bytes in, whatever the codec was ready to hand back out.
    ///
    /// Usually empty until the stream is finished: DEFLATE buffers a window,
    /// and asking for a flush per chunk would change the OUTPUT — which is a
    /// byte count a program reads.
    fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        on_coder!(self, held => {
            let _ = held.write_all(bytes);
            std::mem::take(held.get_mut())
        })
    }

    /// The framing completed, or the error a truncated input is.
    fn finish(self) -> std::io::Result<Vec<u8>> {
        on_coder!(self, held => held.finish())
    }
}

static TABLE: Mutex<Option<HashMap<u64, Coder>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, Coder>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

/// The `CompressionStream` and `DecompressionStream` constructors.
pub(super) fn classes(context: &mut Context) -> (u64, u64) {
    let prototype = compression_prototype(context);
    let compression = super::class_of(context, "CompressionStream", prototype, construct_compression);
    let prototype = decompression_prototype(context);
    let decompression =
        super::class_of(context, "DecompressionStream", prototype, construct_decompression);
    (compression, decompression)
}

fn compression_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "CompressionStream", METHODS)
}

fn decompression_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "DecompressionStream", METHODS)
}

/// The three format names the standard defines, in each direction.
fn coder_for(format: &str, compressing: bool) -> Option<Coder> {
    let level = Compression::default();
    match (format, compressing) {
        ("gzip", true) => Some(Coder::Gzip(GzEncoder::new(Vec::new(), level))),
        ("deflate", true) => Some(Coder::Zlib(ZlibEncoder::new(Vec::new(), level))),
        ("deflate-raw", true) => Some(Coder::Raw(DeflateEncoder::new(Vec::new(), level))),
        ("gzip", false) => Some(Coder::Gunzip(GzDecoder::new(Vec::new()))),
        ("deflate", false) => Some(Coder::Inflate(ZlibDecoder::new(Vec::new()))),
        ("deflate-raw", false) => Some(Coder::RawInflate(DeflateDecoder::new(Vec::new()))),
        _ => None,
    }
}

/// Both constructors, which differ only in direction and prototype.
fn build(this: u64, format: u64, compressing: bool) -> u64 {
    let asked = entry::text_of(format).unwrap_or_default();
    let Some(coder) = coder_for(&asked, compressing) else {
        // A real throw, not an inert instance: the standard makes this a
        // `TypeError`, a native may raise (rule 8 of `rts-core`'s README), and
        // a stream that answers nothing would be the surface-that-cannot-do-
        // what-its-name-means this repository refuses.
        entry::throw_type_error(&format!("Unsupported compression format: {asked}"));
        return entry::undefined_value();
    };
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| table.insert(id, coder));
    entry::with_runtime(|context| {
        let prototype = match compressing {
            true => compression_prototype(context),
            false => decompression_prototype(context),
        };
        let instance = super::self_or_new(context, this, prototype);
        let sink = transform::pair(context, instance, codec_write, codec_close);
        let held = entry::make_number(id as f64);
        entry::put_member(context, sink, CODEC, held);
        instance
    })
}

/// The [`TABLE`] key a sink carries, as a number.
fn key_of(sink: u64) -> Option<u64> {
    entry::number_of(field(sink, CODEC)).map(|number| number as u64)
}

/// Hands a chunk of bytes to the readable half, unless there are none.
fn emit(sink: u64, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let held = entry::with_runtime(|context| entry::make_bytes(context, bytes));
    super::readable::enqueue(field(sink, transform::READABLE), held);
}

// --------------------------------------------------------------- the natives

extern "C" fn construct_compression(_e: u64, this: u64, format: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    build(this, format, true)
}

extern "C" fn construct_decompression(_e: u64, this: u64, format: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    build(this, format, false)
}

/// One chunk of bytes through the codec. `this` is the sink.
extern "C" fn codec_write(_e: u64, this: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(id) = key_of(this) else {
        return absent;
    };
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, chunk)).unwrap_or_default();
    let out = with_table(|table| table.get_mut(&id).map(|coder| coder.feed(&bytes)));
    emit(this, &out.unwrap_or_default());
    absent
}

/// The stream ends: the framing is completed and the last bytes come out.
///
/// A truncated input is what `finish` reports, and it ERRORS the readable half
/// rather than closing it empty — a `DecompressionStream` that answered "no
/// more data" for a corrupt archive would be a wrong answer where the standard
/// has a failure.
extern "C" fn codec_close(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let readable = field(this, transform::READABLE);
    let Some(id) = key_of(this) else {
        super::readable::close(readable);
        return absent;
    };
    match with_table(|table| table.remove(&id)).map(Coder::finish) {
        Some(Err(fault)) => {
            let message = format!("The compressed data was not valid: {fault}");
            let error = entry::make_named_error("TypeError", &message)
                .unwrap_or_else(entry::undefined_value);
            super::readable::fail(readable, error);
        }
        Some(Ok(bytes)) => {
            emit(this, &bytes);
            super::readable::close(readable);
        }
        None => super::readable::close(readable),
    }
    absent
}
