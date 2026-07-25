//! Web Streams — `ReadableStream`/`WritableStream`/`TransformStream` family
//! (9 classes), reimplemented as `#[rtse::class]` Rust per DRAIN_MOTOR §11
//! ("MIGRAR antes de deletar"). Parity source: the LIVE
//! `crates/rts-shared/src/stdlib/streams.ts` (which this module replaces) —
//! plus `CompressionStream`, which the `.ts` never had; ported instead from
//! the git-history hand-written Rust (`git show
//! 7e4868d8^:crates/rts-std/src/globals/readable_stream/instance.rs`, the
//! last commit before the drain-campaign sweep deleted it).
//!
//! ## Model
//!
//! - `ReadableStreamDefaultController` owns a chunk queue (`Vec<u64>`
//!   PolyValue words), OR forwards synchronously into a downstream
//!   `WritableStream` once `pipeThrough` wires one (`fwd`) — mirrors the
//!   `.ts`'s lazy-forwarding controller (drain what's queued, then forward
//!   every later `enqueue` — the common "pipe before you write" order).
//! - `WritableStream` folds the `.ts`'s internal `__StreamSink` DIRECTLY (no
//!   separate class — nothing in JS ever constructs a bare sink) behind a
//!   `mode` tag (identity/transform/encode/decode) that drives `write()`.
//! - `TransformStream`/`TextEncoderStream`/`TextDecoderStream` are thin ctors
//!   wiring a fresh `readable`+`writable` pair whose writable feeds the
//!   readable's controller.
//! - `CompressionStream` reuses the same writable/readable pair with
//!   `mode = Compress`: raw bytes accumulate in the writable side, and
//!   `close()` gzip/deflates them (`flate2`) into the readable side —
//!   exactly the old hand-written Rust's behavior.
//!
//! ## Sync, not async (matches the `.ts` it replaces)
//!
//! `read()`/`write()`/`close()` return the value/`undefined` DIRECTLY — no
//! `Promise` wrapper. This matches the CURRENT `.ts` (which relies on the
//! engine's non-`Promise` `await` passthrough), not the older git-history
//! Rust (which wrapped every return in an already-fulfilled `Promise`). Per
//! the task's own instruction ("do NOT invent async where the old code was
//! sync"): the code this module REPLACES (the live `.ts`) is sync, so this
//! stays sync too.
//!
//! ## Duck-typed user callbacks
//!
//! `source.start`, `transformer.transform`/`flush` are arbitrary
//! user-supplied JS values (object literals or class instances) — read
//! generically via `__rtsadp_obj_get` and invoked via
//! `__rtsadp_fn_invoke_method` (so `this` binds to the callback's own object,
//! matching `t.transform(...)`/`source.start(...)`), the same bridge
//! `text_encoding`/`blob` already use for options bags.

pub mod compression_stream;
pub mod controller;
pub mod readable;
pub mod reader;
pub mod text_decoder_stream;
pub mod text_encoder_stream;
pub mod transform;
pub mod writable;
pub mod writer;

pub use compression_stream::register as register_compression_stream_class_spec;
pub use controller::register as register_controller_class_spec;
pub use readable::register as register_readable_stream_class_spec;
pub use reader::register as register_reader_class_spec;
pub use text_decoder_stream::register as register_text_decoder_stream_class_spec;
pub use text_encoder_stream::register as register_text_encoder_stream_class_spec;
pub use transform::register as register_transform_stream_class_spec;
// `WritableStream` reopens the macro `register` to append the hand-written
// `getWriter` (needs the receiver's OWN handle to back-reference from the
// writer it returns — the macro has no way to expose `__recv` to a method
// body; see `writable.rs`'s doc comment).
pub use writable::register_writable_stream_class_spec;
pub use writer::register as register_writer_class_spec;

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};
use rts_engine::heap::poly::{POLY_FALSE, POLY_NULL, POLY_TRUE, POLY_UNDEFINED, poly_handle_normalize};
use rts_engine::heap::shapes::{alloc_shaped_object, handle_word_auto};

unsafe extern "C" {
    fn __rtsadp_obj_get(obj_word: u64, key_word: u64) -> u64;
    fn __rtsadp_fn_invoke_method(fn_word: u64, this_word: u64, a0: u64, a1: u64, a2: u64, rest: u64) -> u64;
    fn __rtsadp_to_string(word: u64) -> u64;
}

/// Box a plain `&str` literal into a fresh interned string PolyValue word (a
/// property-name lookup key) — same pattern `text_encoding::opt_flag` uses.
pub(crate) fn key_word(name: &str) -> u64 {
    let h = alloc_entry(Entry::String(name.as_bytes().to_vec()));
    handle_word_auto(h)
}

/// `true` unless `w` is `undefined`/`null` (the `.ts`'s repeated
/// `!== undefined && !== null` guard).
pub(crate) fn is_nullish(w: u64) -> bool {
    w == POLY_UNDEFINED || w == POLY_NULL || w == 0
}

/// `obj.key` as a raw PolyValue word (`undefined` if `obj` is nullish or the
/// key is absent) — a generic dynamic property read that works on ANY JS
/// value (object literal, class instance, or one of our own `Entry::Rtse`
/// classes).
pub(crate) fn get_prop(obj_word: u64, key: &str) -> u64 {
    if is_nullish(obj_word) {
        return POLY_UNDEFINED;
    }
    unsafe { __rtsadp_obj_get(obj_word, key_word(key)) }
}

/// Calls `recv.method(args...)` iff `recv.method` is present (duck-typed
/// `"method" in recv`), with `this = recv` — mirrors the `.ts`'s
/// `if ("start" in source) { source.start(ctl); }` pattern. Up to 3 args (the
/// most any stream callback takes); unused slots ride as `undefined`.
pub(crate) fn call_if_present(recv_word: u64, method: &str, args: &[u64]) -> Option<u64> {
    let f = get_prop(recv_word, method);
    if is_nullish(f) {
        return None;
    }
    let a = |i: usize| args.get(i).copied().unwrap_or(POLY_UNDEFINED);
    Some(unsafe { __rtsadp_fn_invoke_method(f, recv_word, a(0), a(1), a(2), POLY_UNDEFINED) })
}

/// The JS string form of `w` (`ToString`), as a Rust `String`.
pub(crate) fn js_to_string(w: u64) -> String {
    let sw = unsafe { __rtsadp_to_string(w) };
    let Some(h) = poly_handle_normalize(sw) else {
        return String::new();
    };
    with_entry(h, |e| match e {
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    })
}

/// A poly-boxed HANDLE word (object/string/function tag) → its underlying
/// `HandleTable` slot, or `0` if `w` isn't a boxed handle (primitive/nullish).
pub(crate) fn handle_of(w: u64) -> u64 {
    poly_handle_normalize(w).unwrap_or(0)
}

/// Decode a "bytes-like" chunk (a `Buffer`/`String`/plain `number[]`
/// `Entry::Vec`, or `0`) into raw bytes — the input shape `write()` accepts
/// for `mode = Compress`/`Decode`.
pub(crate) fn chunk_bytes(word: u64) -> Vec<u8> {
    let Some(h) = poly_handle_normalize(word) else {
        return Vec::new();
    };
    with_entry(h, |e| match e {
        Some(Entry::Buffer(b)) | Some(Entry::String(b)) => b.clone(),
        Some(Entry::Vec(v)) => v.iter().map(|&x| x as u8).collect(),
        _ => Vec::new(),
    })
}

/// A plain `number[]` (`Entry::Vec` of raw byte values) from `bytes` — same
/// convention `blob::text_to_byte_array` uses (a "Uint8Array-like" byte
/// source, not yet a real `ArrayBuffer`/typed array).
pub(crate) fn bytes_to_array(bytes: &[u8]) -> u64 {
    let v: Vec<i64> = bytes.iter().map(|&b| b as i64).collect();
    alloc_entry(Entry::Vec(Box::new(v)))
}

/// Box a fresh `{value, done}` result object for `reader.read()` — a real
/// SHAPED object (`alloc_shaped_object`, same mechanism `Promise.allSettled`'s
/// `{status, value}` rows use — see `promise::allsettled_row`), NOT an
/// `Entry::Map`: `value_word` is already a genuine current-model PolyValue
/// word (arrived via a `Poly` param), and a shaped object's slots store a
/// word VERBATIM — no re-decoding heuristic that an `Entry::Map` slot would
/// need to distinguish an already-tagged word from a natively-written raw one.
pub(crate) fn make_result(value_word: u64, done: bool) -> u64 {
    let done_word = if done { POLY_TRUE } else { POLY_FALSE };
    // Returns the raw HANDLE (not a poly word) — matches `reader::read`'s
    // `Handle` return type; the engine boxes a `Handle` return into a poly
    // word at the call boundary automatically (same convention every other
    // `-> Handle` member in this codebase relies on).
    alloc_shaped_object(&["value", "done"], &[value_word as i64, done_word as i64])
}
