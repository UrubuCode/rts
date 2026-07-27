//! `TextEncoder` global class — encode a JS string to UTF-8 bytes.
//!
//! Migrated (DRAIN_MOTOR §8-9) from the hand-written builder to `#[rtse::class]`:
//! `new`/`encode`/`encodeInto` are all mechanical (the macro's own `&str`/`Handle`
//! marshalling covers every param/return here — no macro gap). The prior
//! stateless `Entry::Env(vec![1])` "token" storage becomes a genuine (empty) Rust
//! struct stored in the generic `Entry::Rtse`; behavior is unchanged (TextEncoder
//! is always UTF-8, no constructor args).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{Entry, alloc_entry, with_entry_mut};

/// `TextEncoder` has no instance state (always UTF-8) — a unit struct instead of
/// the old `Entry::Env(vec![1])` sentinel-vec token.
#[rtse::class("TextEncoder")]
#[derive(Clone)]
pub struct TextEncoder;

#[rtse::class("TextEncoder")]
impl TextEncoder {
    /// `new TextEncoder()` — no args (always UTF-8).
    #[rtse::ctor]
    fn new() -> Self {
        TextEncoder
    }

    /// `encoder.encode(text)` — UTF-8 bytes as a Buffer handle.
    #[rtse::method]
    fn encode(self: &TextEncoder, text: &str) -> Handle {
        alloc_entry(Entry::Buffer(text.as_bytes().to_vec()))
    }

    /// `encoder.encodeInto(src, dest)` — encode `src` as UTF-8 into `dest` (a
    /// Vec-backed Uint8Array or a raw `Entry::Buffer`), writing only WHOLE code
    /// points that fit (WHATWG spec). Returns `{ read, written }`: `read` counts
    /// UTF-16 code units consumed, `written` the bytes stored.
    #[rtse::method(name = "encodeInto")]
    fn encode_into(self: &TextEncoder, src: &str, dest: Handle) -> Handle {
        let (read, written) = with_entry_mut(dest, |entry| {
            let mut read = 0i64;
            let mut w = 0usize;
            let mut tmp = [0u8; 4];
            match entry {
                Some(Entry::Vec(v)) => {
                    let cap = v.len();
                    for ch in src.chars() {
                        let enc = ch.encode_utf8(&mut tmp).as_bytes();
                        if w + enc.len() > cap {
                            break;
                        }
                        for &b in enc {
                            // Slot format matches the TypedArray ctor: an f64 word.
                            v[w] = (b as f64).to_bits() as i64;
                            w += 1;
                        }
                        read += ch.len_utf16() as i64;
                    }
                }
                Some(Entry::Buffer(buf)) => {
                    let cap = buf.len();
                    for ch in src.chars() {
                        let enc = ch.encode_utf8(&mut tmp).as_bytes();
                        if w + enc.len() > cap {
                            break;
                        }
                        buf[w..w + enc.len()].copy_from_slice(enc);
                        w += enc.len();
                        read += ch.len_utf16() as i64;
                    }
                }
                _ => {}
            }
            (read, w as i64)
        });
        let mut obj: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
        obj.insert("read".to_string(), read);
        obj.insert("written".to_string(), written);
        alloc_entry(Entry::Map(Box::new(obj)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_roundtrip_len() {
        let h = __rtsm_global_textencoder_new();
        let bh = __rtsm_global_textencoder_encode(
            h,
            "hi".as_ptr() as i64,
            2,
        );
        let bytes = rts_engine::heap::handles::with_entry(bh, |e| match e {
            Some(Entry::Buffer(v)) => v.clone(),
            _ => Vec::new(),
        });
        assert_eq!(bytes, b"hi".to_vec());
    }
}
