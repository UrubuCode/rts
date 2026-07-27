//! `TextDecoder` global class — decode a Uint8Array (UTF-8) to a JS string.
//!
//! DRAIN_MOTOR migration: fully `#[rtse::class]`-authored. The ctor is
//! `#[rtse::ctor(optional = 2)]` (`label` + `options` both optional, same
//! arity window `[0,2]` the old hand-written `Sig::with_defaults` gave).
//! `decode` is `#[rtse::method(throws, optional = 2)]` — the `throws` keyword
//! (landed after this migration started) sets `MemberFlags::THROWS` on the
//! generated Member, so the `fatal` option's real `TypeError` (thrown into the
//! engine's pending-error slot) still routes through the post-call check into
//! `try/catch`, exactly like the prior hand-written residual. One storage
//! representation throughout (the generic `Entry::Rtse`), replacing the old
//! `Entry::Env` token-vec (`[kind, fatal, ignoreBOM, started, ...pending]`).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

use super::instance::slot_byte;

/// Read a boolean option `key` from an options-bag handle (a keyed object /
/// `Entry::Map`; `0` = absent bag). Resolves through the engine's own dynamic
/// property read + ToBoolean (link-resolved externs — no crate cycle).
fn opt_flag(opts_h: u64, key: &str) -> bool {
    unsafe extern "C" {
        fn __rtsadp_obj_get(obj_word: u64, key_word: u64) -> u64;
        fn __rtsadp_to_boolean(word: u64) -> u64;
    }
    use rts_engine::heap::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE;
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_TAG_OBJECT, POLY_TAG_SHIFT, POLY_TAG_STR};
    if opts_h == 0 {
        return false;
    }
    let obj_word = POLY_BOX_BASE
        | (POLY_TAG_OBJECT << POLY_TAG_SHIFT)
        | __RTS_FN_NS_GC_POLY_FROM_HANDLE(opts_h);
    let key_h = alloc_entry(Entry::String(key.as_bytes().to_vec()));
    let key_word = POLY_BOX_BASE
        | (POLY_TAG_STR << POLY_TAG_SHIFT)
        | __RTS_FN_NS_GC_POLY_FROM_HANDLE(key_h);
    unsafe { __rtsadp_to_boolean(__rtsadp_obj_get(obj_word, key_word)) != 0 }
}

/// `TextDecoder` instance state — `fatal`/`ignoreBOM` flags (read once from the
/// ctor's options bag) + streaming state (`started`: the current stream already
/// began BOM handling; `pending`: an incomplete UTF-8 tail carried between
/// `decode(chunk, {stream: true})` calls).
#[rtse::class("TextDecoder")]
#[derive(Clone)]
pub struct TextDecoder {
    fatal: bool,
    ignore_bom: bool,
    started: bool,
    pending: Vec<u8>,
}

#[rtse::class("TextDecoder")]
impl TextDecoder {
    /// (cross-runtime #874/#1407) `new TextDecoder(label?, options?)` — `label`
    /// is optional (so' UTF-8; aceito e ignorado) + an optional options-bag
    /// `{fatal, ignoreBOM}` guardado no estado da instância.
    #[rtse::ctor(optional = 2)]
    fn new(label: &str, options: Handle) -> Self {
        let _ = label; // UTF-8 only — accepted, ignored (matches prior behavior).
        TextDecoder {
            fatal: opt_flag(options, "fatal"),
            ignore_bom: opt_flag(options, "ignoreBOM"),
            started: false,
            pending: Vec::new(),
        }
    }

    /// (#1407/#1408) `decoder.decode(buf?, opts?)` — WHATWG semantics:
    /// per-stream BOM strip (unless `ignoreBOM`), `{stream: true}` carrying a
    /// split UTF-8 tail between chunks, a no-arg flush call, and `fatal`
    /// throwing a real `TypeError` on malformed input (`throws` → the engine
    /// routes the post-call pending-error check into `try/catch`).
    // Returns `String` (not a raw `Handle`) so the engine string-boxes the result
    // (ts `): string`); a `Handle` return rides as ts `object` and renders the
    // decoded text as `[object Object]` (the migration regression this fixes).
    #[rtse::method(name = "decode", throws, optional = 2)]
    fn decode(self: &mut TextDecoder, buf: Handle, opts: Handle) -> String {
        let stream = opt_flag(opts, "stream");
        // Drain the instance state: flags + any pending bytes from a prior chunk.
        let fatal = self.fatal;
        let ignore_bom = self.ignore_bom;
        let started = self.started;
        let mut bytes = std::mem::take(&mut self.pending);
        // Append this call's input (absent on the flush call `decode()`).
        if buf != 0 {
            let extra = with_entry(buf, |entry| match entry {
                Some(Entry::Buffer(v)) | Some(Entry::String(v)) => Some(v.clone()),
                Some(Entry::Vec(v)) => Some(v.iter().map(|&x| slot_byte(x)).collect()),
                _ => None,
            });
            if let Some(b) = extra {
                bytes.extend_from_slice(&b);
            }
        }
        // Streaming: hold back a potentially-valid incomplete UTF-8 tail.
        let mut hold: Vec<u8> = Vec::new();
        if stream {
            let cut = utf8_complete_prefix_len(&bytes);
            hold = bytes.split_off(cut);
        }
        // BOM: stripped ONCE at the start of each stream, unless ignoreBOM. The
        // stream only "starts" when bytes are actually EMITTED — a fully held-back
        // tail (a BOM split across chunks) keeps the strip armed for the next call.
        let mut emit: &[u8] = &bytes;
        let mut new_started = started;
        if !bytes.is_empty() {
            new_started = true;
        }
        if !started && !ignore_bom && emit.starts_with(&[0xEF, 0xBB, 0xBF]) {
            emit = &emit[3..];
        }
        // Decode: fatal throws TypeError on malformed input (WHATWG); otherwise
        // U+FFFD replacement. The flush call also fails a fatal decoder whose
        // pending tail never completed.
        let text: String = match std::str::from_utf8(emit) {
            Ok(s) => s.to_owned(),
            Err(_) if fatal => {
                // Throw into the ENGINE's pending-error slot (a real TypeError
                // instance, so `e instanceof TypeError` / `e.constructor.name`
                // hold) — link-reached extern, paired with the member's `throws` flag.
                unsafe extern "C" {
                    fn __rtsadp_throw_js_error(
                        kind_ptr: *const u8,
                        kind_len: i64,
                        msg_ptr: *const u8,
                        msg_len: i64,
                    );
                }
                let kind = "TypeError";
                let msg = "The encoded data was not valid utf-8";
                unsafe {
                    __rtsadp_throw_js_error(
                        kind.as_ptr(),
                        kind.len() as i64,
                        msg.as_ptr(),
                        msg.len() as i64,
                    );
                }
                // Matches the pre-migration behavior: a fatal throw does NOT
                // persist `started`/`pending` (the stream state before this
                // call stays as-is — the decoder is expected to be discarded
                // or re-driven from the same point after the caught error).
                return String::new();
            }
            Err(_) => String::from_utf8_lossy(emit).into_owned(),
        };
        // Persist / reset the stream state.
        if stream {
            self.started = new_started;
            self.pending = hold;
        } else {
            self.started = false; // a non-stream decode ends the stream
        }
        // A FINAL (non-stream) call with an incomplete tail still pending would
        // have kept it in `emit` (no cut) — already surfaced above.
        text
    }
}

/// The length of the maximal WELL-FORMED-so-far prefix of `bytes`: everything
/// except a trailing incomplete (but still potentially valid) UTF-8 sequence.
/// A trailing sequence that can never become valid is NOT held back — it stays
/// in the prefix so the caller surfaces it (U+FFFD / fatal TypeError) now.
fn utf8_complete_prefix_len(bytes: &[u8]) -> usize {
    let n = bytes.len();
    // A lead byte at most 3 positions from the end can still be incomplete.
    let start = n.saturating_sub(3);
    for i in (start..n).rev() {
        let b = bytes[i];
        let need = match b {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => continue, // continuation/ASCII/invalid — not a lead byte
        };
        let have = n - i;
        if have < need && bytes[i + 1..].iter().all(|&c| (0x80..=0xBF).contains(&c)) {
            return i; // a potentially-valid incomplete tail — hold it back
        }
        return n; // the last lead's sequence is complete (or already invalid)
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::alloc_rtse;

    fn dec(fatal: bool) -> u64 {
        alloc_rtse("TextDecoder", TextDecoder {
            fatal,
            ignore_bom: false,
            started: false,
            pending: Vec::new(),
        })
    }

    #[test]
    fn decode_plain_bytes() {
        let d = dec(false);
        let buf = alloc_entry(Entry::Buffer(b"hi".to_vec()));
        let sh = __rtsm_global_textdecoder_decode(d, buf, 0);
        let s = with_entry(sh, |e| match e {
            Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        });
        assert_eq!(s, "hi");
    }

    #[test]
    fn decode_strips_bom_once() {
        let d = dec(false);
        let buf = alloc_entry(Entry::Buffer([0xEFu8, 0xBB, 0xBF, b'x'].to_vec()));
        let sh = __rtsm_global_textdecoder_decode(d, buf, 0);
        let s = with_entry(sh, |e| match e {
            Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        });
        assert_eq!(s, "x");
    }
}
