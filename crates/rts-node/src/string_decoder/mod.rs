//! `node:string_decoder` — the full surface: the single `StringDecoder` class
//! (`new StringDecoder([encoding])`, `.write(buffer)`, `.end([buffer])`,
//! `.encoding`), a faithful native port of Node's incremental decoder. No
//! stubs — real multi-byte/multi-unit boundary handling for utf8/utf16le/
//! base64/base64url/latin1/ascii/hex.
//!
//! `StringDecoder` is registered as an object-backed Registry class (the same
//! model as `DOMException`): the instance is an `Entry::Map` tagged
//! `__rts_class = "StringDecoder"`, so method dispatch and `new` resolve
//! data-driven, the engine naming no non-primordial. Members carry an explicit
//! `ts_signature` so a `Handle` string return reboxes as a string (not an
//! opaque object). A `node:string_decoder` namespace is registered so the
//! module specifier resolves.

mod class;
mod decoder;
use rts_engine::Engine;

/// Registers the `StringDecoder` class + the `node:string_decoder` module.
///
/// The class is DECLARED in [`class`] with `#[rtse::class]`, which generates
/// every ABI symbol, both ctor overloads, the methods, the getter and the
/// `register` below — replacing ~110 lines of hand-written `Member` rows plus a
/// local `member(...)` builder that restated each symbol name and signature.
pub fn register(e: &mut Engine) {
    class::register(e);

    e.module("node:string_decoder", |m| {
        m.doc("Incremental byte→string decoder. Exports the StringDecoder class.");
        // `StringDecoder` is a registered global (Registry) class — bind the
        // import to the ambient class, like `URL` (reuse, never re-implement).
        m.reexport("StringDecoder", "StringDecoder");
    });
}
