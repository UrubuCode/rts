//! One codec path, used by both the one-shot functions and the streaming
//! classes.
//!
//! # Why one path and not two
//!
//! The obvious split is "`flate2::read::GzDecoder` for `gunzipSync`,
//! `flate2::write::GzDecoder` for the `Gunzip` stream" — which is what the
//! first version of this module did for the sync half. Two paths mean two
//! answers to what a truncated input produces, what `maxOutputLength` counts,
//! and which options are honoured; the sync half already diverged from the
//! spec's §4 on the first of those. So the one-shot functions are a
//! [`Sink`] fed once and finished, and the streams are the same [`Sink`] fed
//! many times. The cost is that the one-shot form allocates a `Vec` sink it
//! could have skipped — paid deliberately, because a second decoder is how
//! the two come to disagree.
//!
//! # Why `write::` adapters rather than `flate2::Compress`
//!
//! `Compress`/`Decompress` are the low-level state machines, and they hand
//! back a `Status` per call that the caller must loop on and grow an output
//! buffer for. That loop is exactly what the `write::` adapters already are,
//! over a `Vec<u8>` this module drains after each feed. Reimplementing it
//! here would be a second copy of `flate2`'s own output pump.

use std::io::Write;

use flate2::Compression;
use flate2::write::{DeflateDecoder, DeflateEncoder, GzDecoder, GzEncoder, ZlibDecoder, ZlibEncoder};

/// Which of Node's nine non-Zstd codecs a call is.
///
/// A data discriminant rather than nine copies of the same function body —
/// the same reason `docs/reference/node/zlib.md` §5.2 gives for its `kind`
/// value, kept even though the ABI that section describes does not apply
/// here.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Deflate,
    Inflate,
    DeflateRaw,
    InflateRaw,
    Gzip,
    Gunzip,
    Unzip,
    BrotliCompress,
    BrotliDecompress,
}

impl Kind {
    /// The Node class name for this codec — the prototype name a streaming
    /// instance is registered under, so `Object.getPrototypeOf` answers one
    /// object per class rather than one per `createGzip()` call.
    pub(super) fn class_name(self) -> &'static str {
        match self {
            Kind::Deflate => "Deflate",
            Kind::Inflate => "Inflate",
            Kind::DeflateRaw => "DeflateRaw",
            Kind::InflateRaw => "InflateRaw",
            Kind::Gzip => "Gzip",
            Kind::Gunzip => "Gunzip",
            Kind::Unzip => "Unzip",
            Kind::BrotliCompress => "BrotliCompress",
            Kind::BrotliDecompress => "BrotliDecompress",
        }
    }
}

/// The tuning knobs this module actually acts on.
///
/// Deliberately smaller than `ZlibOptions`: see the module doc of `mod.rs`
/// for which fields are refused by name and why. A struct of what is honoured
/// beats a struct of everything documented with half the fields ignored,
/// because the second shape reads as support.
#[derive(Clone)]
pub(super) struct Settings {
    /// `level`, already clamped to 0..=9 (`Z_DEFAULT_COMPRESSION` resolved).
    pub(super) level: u32,
    /// `chunkSize` — the encoder's internal buffer, not an output cap.
    pub(super) chunk_size: usize,
    /// `maxOutputLength`; `None` means unbounded.
    pub(super) max_output: Option<usize>,
    /// `finishFlush: Z_SYNC_FLUSH` — return partial data instead of failing
    /// on a truncated stream (spec §4's documented relaxation).
    pub(super) tolerant: bool,
    /// `params[BROTLI_PARAM_QUALITY]`.
    pub(super) quality: u32,
    /// `params[BROTLI_PARAM_LGWIN]`.
    pub(super) lgwin: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            level: 6,
            chunk_size: 16 * 1024,
            max_output: None,
            tolerant: false,
            quality: 11,
            lgwin: 22,
        }
    }
}

/// What every backing writer has in common: an output `Vec` to drain, and a
/// terminal step that either completes the framing or reports that the input
/// ended early.
///
/// `Err` carries the bytes recovered so far rather than nothing, because
/// `finishFlush: Z_SYNC_FLUSH` asks for exactly those — a `Result<Vec, ()>`
/// would have thrown away the only thing that answer needs.
trait Codec: Write {
    fn output(&mut self) -> &mut Vec<u8>;
    fn done(self: Box<Self>) -> Result<Vec<u8>, Vec<u8>>;
}

macro_rules! flate_codec {
    ($type:ty) => {
        impl Codec for $type {
            fn output(&mut self) -> &mut Vec<u8> {
                self.get_mut()
            }
            fn done(self: Box<Self>) -> Result<Vec<u8>, Vec<u8>> {
                let mut held = *self;
                // Drained BEFORE `finish` consumes the writer: `finish`
                // answers the sink it owns, and on the error path it answers
                // nothing at all — so anything already produced would be lost
                // exactly in the truncation case that wants it.
                let partial = std::mem::take(held.get_mut());
                match held.finish() {
                    Ok(mut rest) => {
                        let mut all = partial;
                        all.append(&mut rest);
                        Ok(all)
                    }
                    Err(_) => Err(partial),
                }
            }
        }
    };
}

flate_codec!(GzEncoder<Vec<u8>>);
flate_codec!(GzDecoder<Vec<u8>>);
flate_codec!(ZlibEncoder<Vec<u8>>);
flate_codec!(ZlibDecoder<Vec<u8>>);
flate_codec!(DeflateEncoder<Vec<u8>>);
flate_codec!(DeflateDecoder<Vec<u8>>);

impl Codec for brotli::CompressorWriter<Vec<u8>> {
    fn output(&mut self) -> &mut Vec<u8> {
        self.get_mut()
    }
    fn done(self: Box<Self>) -> Result<Vec<u8>, Vec<u8>> {
        let mut held = *self;
        let partial = std::mem::take(held.get_mut());
        // `into_inner` performs BROTLI_OPERATION_FINISH — this is the call
        // that writes the terminal block, not a plain accessor.
        let mut rest = held.into_inner();
        let mut all = partial;
        all.append(&mut rest);
        Ok(all)
    }
}

impl Codec for brotli::DecompressorWriter<Vec<u8>> {
    fn output(&mut self) -> &mut Vec<u8> {
        self.get_mut()
    }
    fn done(self: Box<Self>) -> Result<Vec<u8>, Vec<u8>> {
        let mut held = *self;
        let partial = std::mem::take(held.get_mut());
        // `Err` here means the brotli stream never reached its final block —
        // a truncated input, which is the one distinction this whole trait
        // exists to preserve.
        match held.into_inner() {
            Ok(mut rest) => {
                let mut all = partial;
                all.append(&mut rest);
                Ok(all)
            }
            Err(mut rest) => {
                let mut all = partial;
                all.append(&mut rest);
                Err(all)
            }
        }
    }
}

fn writer_for(kind: Kind, settings: &Settings) -> Box<dyn Codec + Send> {
    let level = Compression::new(settings.level);
    let buffer = settings.chunk_size.max(1024);
    match kind {
        Kind::Gzip => Box::new(GzEncoder::new(Vec::new(), level)),
        Kind::Gunzip => Box::new(GzDecoder::new(Vec::new())),
        Kind::Deflate => Box::new(ZlibEncoder::new(Vec::new(), level)),
        Kind::Inflate => Box::new(ZlibDecoder::new(Vec::new())),
        Kind::DeflateRaw => Box::new(DeflateEncoder::new(Vec::new(), level)),
        Kind::InflateRaw => Box::new(DeflateDecoder::new(Vec::new())),
        Kind::BrotliCompress => Box::new(brotli::CompressorWriter::new(
            Vec::new(),
            buffer,
            settings.quality,
            settings.lgwin,
        )),
        Kind::BrotliDecompress => Box::new(brotli::DecompressorWriter::new(Vec::new(), buffer)),
        // Never reached: `Unzip` has no writer until the header is seen, and
        // `Sink::new` leaves it absent rather than guessing. Answering the
        // gzip decoder here would be that guess, wrong for half its inputs.
        Kind::Unzip => Box::new(GzDecoder::new(Vec::new())),
    }
}

/// A codec being fed, over its accumulated output.
pub(super) struct Sink {
    settings: Settings,
    codec: Option<Box<dyn Codec + Send>>,
    /// Bytes held back while `Unzip` waits for enough header to decide.
    pending: Vec<u8>,
}

impl Sink {
    pub(super) fn new(kind: Kind, settings: &Settings) -> Sink {
        let codec = match kind {
            Kind::Unzip => None,
            other => Some(writer_for(other, settings)),
        };
        Sink { settings: settings.clone(), codec, pending: Vec::new() }
    }

    /// Feeds input. `Err` means the codec rejected it — corrupt framing, or
    /// data that is not what this decoder decodes.
    pub(super) fn feed(&mut self, bytes: &[u8]) -> Result<(), ()> {
        if self.codec.is_none() {
            self.pending.extend_from_slice(bytes);
            // TWO bytes is the whole decision: `1f 8b` is gzip's magic, and
            // anything else with a valid zlib CMF/FLG is the zlib framing.
            // Deciding on one byte would call every zlib stream starting
            // `0x1f` a gzip stream.
            if self.pending.len() < 2 {
                return Ok(());
            }
            let held = std::mem::take(&mut self.pending);
            let sniffed = match held.starts_with(&[0x1f, 0x8b]) {
                true => Kind::Gunzip,
                false => Kind::Inflate,
            };
            self.codec = Some(writer_for(sniffed, &self.settings));
            let Some(codec) = self.codec.as_mut() else {
                return Err(());
            };
            return codec.write_all(&held).map_err(|_| ());
        }
        let Some(codec) = self.codec.as_mut() else {
            return Err(());
        };
        codec.write_all(bytes).map_err(|_| ())
    }

    /// Everything produced so far, removed from the sink.
    pub(super) fn drain(&mut self) -> Vec<u8> {
        match self.codec.as_mut() {
            Some(codec) => std::mem::take(codec.output()),
            None => Vec::new(),
        }
    }

    /// A mid-stream flush — `zlibBase.flush()`. Answers whatever it shook
    /// loose, which for a compressor is a real block boundary and for a
    /// decompressor is usually nothing.
    pub(super) fn flush_now(&mut self) -> Vec<u8> {
        let Some(codec) = self.codec.as_mut() else {
            return Vec::new();
        };
        let _ = codec.flush();
        std::mem::take(codec.output())
    }

    /// Ends the stream. `None` when the input ended early and the caller did
    /// not ask for partial data — never an empty `Vec`, which is a legitimate
    /// result (the gzip of `""` decompresses to nothing) and so cannot double
    /// as a failure.
    pub(super) fn finish(self) -> Option<Vec<u8>> {
        let Some(codec) = self.codec else {
            // `Unzip` that never saw two bytes. There is no framing to
            // decide, so there is no answer — not an empty one.
            return match self.pending.is_empty() && self.settings.tolerant {
                true => Some(Vec::new()),
                false => None,
            };
        };
        match codec.done() {
            Ok(bytes) => Some(bytes),
            Err(partial) => match self.settings.tolerant {
                true => Some(partial),
                false => None,
            },
        }
    }
}

/// One buffer in, one buffer out — what every `*Sync` function is.
///
/// `None` for a failure, and the `maxOutputLength` cap is enforced here
/// because Node documents it as a convenience-function option (§3).
pub(super) fn one_shot(kind: Kind, input: &[u8], settings: &Settings) -> Option<Vec<u8>> {
    let mut sink = Sink::new(kind, settings);
    let mut out = Vec::new();
    if sink.feed(input).is_err() && !settings.tolerant {
        return None;
    }
    out.append(&mut sink.drain());
    let mut tail = sink.finish()?;
    out.append(&mut tail);
    match settings.max_output {
        Some(cap) if out.len() > cap => None,
        _ => Some(out),
    }
}

/// CRC-32 (IEEE), seeded — `zlib.crc32(data, value)`.
///
/// # Reuse check
///
/// `crc32fast` is what `docs/reference/node/zlib.md` §5.1 names, and it is
/// NOT a dependency of this crate; adding it means editing `Cargo.toml`,
/// which this module does not own (a manifest is where parallel work
/// collides — the comment at the top of that file says so). `flate2::Crc` is
/// already reachable and was the first candidate, but it has only `new()`:
/// there is no seeded constructor, so the chained form Node documents
/// (`crc32(second, crc32(first))`) cannot be expressed through it, and a
/// `crc32` that silently ignores its second argument is exactly the wrong
/// answer that runs. So: the bitwise reference algorithm, twelve lines, no
/// table. It is slower than a table-driven or SIMD implementation and that
/// is a stated trade, not an oversight.
pub(super) fn crc32(bytes: &[u8], seed: u32) -> u32 {
    let mut crc = !seed;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let carry = crc & 1;
            crc >>= 1;
            if carry != 0 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}
