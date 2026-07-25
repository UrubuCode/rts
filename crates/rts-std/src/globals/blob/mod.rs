//! `Blob` + `File` global classes (#74/#75).
//!
//! DRAIN_MOTOR §11 (owner 2026-07-24): reimplemented as `#[rtse::class]`,
//! ported from the deleted hand-written builder (`git show HEAD~1:crates/
//! rts-std/src/globals/blob/mod.rs` before this session's sweep) AND brought
//! up to parity with the richer LIVE `.ts` surface
//! (`crates/rts-shared/src/stdlib/webapi.ts`'s `class Blob`/`class File`),
//! which is what the loader/engine actually exercises today (`type` getter,
//! `arrayBuffer()`, `slice()`, `File`'s `opts.lastModified` fallback to
//! `Date.now()`). One class per file (`blob.rs`/`file.rs`) — two
//! `#[rtse::class]` in one file collide on the generated `pub fn register`.
//!
//! Storage: a normal Rust `String` (the DECODED text, matching webapi.ts's
//! model — a `Blob` normalizes every part to UTF-8 text at construction, so
//! `size`/`text`/`arrayBuffer`/`slice` all derive from one representation) via
//! the generic `Entry::Rtse` (same pattern as `Headers`/`UrlSearchParams`).
//!
//! `File extends Blob`: composition + forwarding (DRAIN_MOTOR §11 "Herança"
//! decision) — `File` embeds a `Blob` field and forwards `size`/`type`/`text`/
//! `arrayBuffer`/`slice`; `extends = "Blob"` links the Registry parent so
//! `file instanceof Blob` resolves via the class hierarchy.
//!
//! **Known gap vs the `.ts`:** `Blob.stream()` (a `ReadableStream` instance) is
//! NOT reimplemented — constructing a live `.ts` class instance (`new
//! ReadableStream()`) from a native `#[rtse::class]` body has no bridge today
//! (out of scope for this port; the task's parity list doesn't ask for it).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};
use rts_engine::heap::poly::{
    POLY_BOX_BASE, POLY_TAG_OBJECT, POLY_TAG_SHIFT, POLY_TAG_STR, POLY_UNDEFINED, poly_handle_normalize,
    poly_number,
};
use rts_engine::heap::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE;

pub mod blob;
pub mod file;

pub use blob::register_blob_class_spec;
pub use file::register_file_class_spec;

/// UTF-8-decode arbitrary bytes (matches webapi.ts's `__utf8_decode`; lossy on
/// invalid sequences, same as the rest of the codebase's string decoding).
pub(crate) fn utf8_decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Decode ONE `BlobPart` element (a string handle, a raw byte buffer, or a
/// number-array "byte source") into text — the same normalization
/// `webapi.ts`'s ctor loop does (`typeof p === "string" ? p : __utf8_decode(p)`).
/// A nested `Blob`/`File` part reuses its own already-decoded text.
fn decode_part(h: u64) -> String {
    use rts_engine::heap::handles::with_rtse;
    if h == 0 {
        return String::new();
    }
    // A JS array element is a boxed PolyValue WORD (string/object), not a raw
    // handle — normalize it to the underlying HandleTable handle before reading.
    let h = rts_engine::heap::poly::poly_handle_normalize(h).unwrap_or(h);
    // A nested `Blob`/`File` part reuses its own already-decoded text.
    if let Some(t) = with_rtse::<blob::Blob, _>(h, |s| s.map(|b| b.text_ref().to_string())) {
        return t;
    }
    if let Some(t) = with_rtse::<file::File, _>(h, |s| s.map(|f| f.text_ref().to_string())) {
        return t;
    }
    with_entry(h, |e| match e {
        Some(Entry::String(b)) => utf8_decode(b),
        Some(Entry::Buffer(b)) => utf8_decode(b),
        Some(Entry::Vec(v)) => {
            let bytes: Vec<u8> = v.iter().map(|&x| x as u8).collect();
            utf8_decode(&bytes)
        }
        _ => String::new(),
    })
}

/// Concatenate the decoded text of every `BlobPart` in the `parts` array
/// handle (`0` — `undefined`/absent — is the empty `Blob`).
pub(crate) fn concat_parts(parts_h: Handle) -> String {
    if parts_h == 0 {
        return String::new();
    }
    let elems: Vec<i64> = with_entry(parts_h, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out = String::new();
    for el in elems {
        out.push_str(&decode_part(el as u64));
    }
    out
}

/// Read `obj.key` as a raw PolyValue word (`opts_h == 0` or the key absent →
/// `None`, mirroring `event_target::opt_flag`'s dynamic-property-read pattern).
fn obj_field_word(opts_h: Handle, key: &str) -> Option<u64> {
    unsafe extern "C" {
        fn __rtsadp_obj_get(obj_word: u64, key_word: u64) -> u64;
    }
    if opts_h == 0 {
        return None;
    }
    let obj_word = POLY_BOX_BASE | (POLY_TAG_OBJECT << POLY_TAG_SHIFT) | __RTS_FN_NS_GC_POLY_FROM_HANDLE(opts_h);
    let key_h = alloc_entry(Entry::String(key.as_bytes().to_vec()));
    let key_word = POLY_BOX_BASE | (POLY_TAG_STR << POLY_TAG_SHIFT) | __RTS_FN_NS_GC_POLY_FROM_HANDLE(key_h);
    let val = unsafe { __rtsadp_obj_get(obj_word, key_word) };
    if val == POLY_UNDEFINED { None } else { Some(val) }
}

/// `opts.<key>` as a `String` (only if the property is a genuine string), for
/// reading `{ type: "..." }` from an options object.
pub(crate) fn opt_str(opts_h: Handle, key: &str) -> Option<String> {
    let word = obj_field_word(opts_h, key)?;
    let handle = poly_handle_normalize(word)?;
    with_entry(handle, |e| match e {
        Some(Entry::String(b)) => Some(utf8_decode(b)),
        _ => None,
    })
}

/// `opts.<key>` as a number (only if the property is a genuine number), for
/// reading `{ lastModified: 123 }` from an options object.
pub(crate) fn opt_num(opts_h: Handle, key: &str) -> Option<f64> {
    let word = obj_field_word(opts_h, key)?;
    poly_number(word)
}

/// UTF-8 bytes of `text` as a plain `number[]` (`Entry::Vec` of raw `i64` byte
/// values) — matches `webapi.ts`'s `__utf8_encode` / `arrayBuffer()` model
/// (a "Uint8Array-like" byte source, not yet a real `ArrayBuffer` object).
pub(crate) fn text_to_byte_array(text: &str) -> u64 {
    let bytes: Vec<i64> = text.bytes().map(|b| b as i64).collect();
    alloc_entry(Entry::Vec(Box::new(bytes)))
}

/// `Blob.slice`/`File`-via-`Blob.slice` byte-range extraction. `end < 0` means
/// "to the end" (WHATWG default `-1`). Indexes RAW BYTES of the decoded text
/// (an improvement over the `.ts`'s UTF-16-code-unit slicing, exact for ASCII
/// either way — see `webapi.ts`'s own documented caveat).
pub(crate) fn slice_text(text: &str, start: i64, end: i64) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len() as i64;
    let s = start.clamp(0, len) as usize;
    let e = if end < 0 { len } else { end.clamp(0, len) } as usize;
    if s < e {
        utf8_decode(&bytes[s..e])
    } else {
        String::new()
    }
}
