//! `node:string_decoder`'s `StringDecoder`, authored as a `#[rtse::class]`.
//!
//! # What changed, and why it is simpler
//!
//! The instance used to be an `Entry::Map` tagged `__rts_class`, with the
//! decoder's state flattened into integer/string fields — `enc` as an index,
//! the pending bytes bit-packed into an `i64` — and every method had to
//! `load` that back into a real [`Decoder`], run, then `store` it again.
//!
//! With `#[rtse::class]` the instance IS the Rust struct (`Entry::Rtse`), so it
//! just HOLDS the `Decoder`. The pack/unpack/enc-index round trip disappears,
//! and `&mut self` writes the state back by itself. The struct lives here and
//! the decoder in its own module — a Rust struct can hold a type from anywhere,
//! which is what makes the state model a plain field instead of an encoding.
//!
//! The macro generates every ABI symbol, the ctor overloads, the getter, and
//! `register`; nothing here spells a symbol name or builds a `Member`.

use rts_engine::abi::ty::Handle;

use super::decoder::{Decoder, Enc};
use rts_engine::heap::handles::{Entry, with_entry};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// `TypeError: Unknown encoding: <name>` — Node's `ERR_UNKNOWN_ENCODING`.
fn throw_unknown_encoding(name: &str) {
    let kind = "TypeError";
    let msg = format!("Unknown encoding: {name}");
    unsafe {
        __rtsadp_throw_js_error(
            kind.as_ptr(),
            kind.len() as i64,
            msg.as_ptr(),
            msg.len() as i64,
        );
    }
}

/// A `StringDecoder` instance: the incremental [`Decoder`] itself, no encoding.
#[rtse::class("StringDecoder")]
#[derive(Clone)]
pub struct StringDecoder {
    dec: Decoder,
}

#[rtse::class("StringDecoder")]
impl StringDecoder {
    /// `new StringDecoder()` — defaults to `utf8`.
    #[rtse::ctor]
    fn new() -> Self {
        StringDecoder {
            dec: Decoder::new(Enc::Utf8),
        }
    }

    /// `new StringDecoder(encoding)` — throws `TypeError` on an unknown one.
    #[rtse::ctor(throws)]
    fn with_encoding(encoding: &str) -> Option<Self> {
        match Enc::parse(encoding) {
            Some(enc) => Some(StringDecoder {
                dec: Decoder::new(enc),
            }),
            None => {
                throw_unknown_encoding(encoding);
                None
            }
        }
    }

    /// `decoder.write(buffer)` — decode a chunk, holding back any partial
    /// trailing character for the next call.
    #[rtse::method]
    fn write(&mut self, input: Handle) -> String {
        self.dec.write(&read_bytes(input))
    }

    /// `decoder.end()` — flush, emitting U+FFFD for an incomplete tail.
    #[rtse::method]
    fn end(&mut self) -> String {
        self.dec.end(None)
    }

    /// `decoder.end(buffer)` — write then flush. Shares the JS name `end` with
    /// the zero-arg form; the class path resolves the two by ARITY, and their
    /// symbols differ because the Rust idents do.
    #[rtse::method(name = "end")]
    fn end_buf(&mut self, input: Handle) -> String {
        let bytes = read_bytes(input);
        self.dec.end(Some(&bytes))
    }

    /// `decoder.text(buffer, offset)` — the legacy internal entry point.
    #[rtse::method]
    fn text(&mut self, input: Handle, offset: i64) -> String {
        let bytes = read_bytes(input);
        self.dec.text(&bytes, offset.max(0) as usize)
    }

    /// `decoder.encoding` — the canonical encoding name.
    #[rtse::getter]
    fn encoding(&self) -> String {
        self.dec.enc.name().to_string()
    }
}


/// Read the raw bytes of a `write`/`end` argument:
/// - `Entry::Buffer` (an ArrayBuffer-view typed array) — verbatim bytes;
/// - `Entry::Vec` (the common `new Uint8Array([...])` form — a JS array of
///   boxed number words) — each element's low byte;
/// - `Entry::String` (a JS string) — its UTF-8 bytes (Node's implicit
///   `Buffer.from(string)`).
/// Anything else → empty.
fn read_bytes(handle: u64) -> Vec<u8> {
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};
    with_entry(handle, |e| match e {
        Some(Entry::Buffer(b)) => b.clone(),
        Some(Entry::String(s)) => s.clone(),
        Some(Entry::Vec(v)) => v
            .iter()
            .map(|&w| {
                let u = w as u64;
                if (u & POLY_BOX_BASE) != POLY_BOX_BASE {
                    // inline f64
                    f64::from_bits(u) as u8
                } else {
                    // boxed INT32 (or other) — low payload bits
                    (u & POLY_PAYLOAD_MASK) as u32 as u8
                }
            })
            .collect(),
        _ => Vec::new(),
    })
}
