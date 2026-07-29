//! `node:buffer` — the static / global surface that works without the
//! instance-method wall: `atob`/`btoa` and the `Buffer` STATIC methods
//! (`alloc`/`allocUnsafe`/`from`/`isBuffer`/`byteLength`/`concat`/`compare`/
//! `toString`), which dispatch statically and return `Uint8Array`-shaped
//! arrays. Real bytes.
//!
//! `toString`/`toString(encoding)` are exposed as `Buffer.toString(buf[, enc])`
//! — a STATIC call, not `buf.toString()` — because the engine has no proven
//! class tag for a Buffer's `Entry::Vec` receiver to dispatch a real instance
//! method on (a plain array receiver's `.toString()` resolves to
//! `Array.prototype.toString` — comma-joined — long before it could reach a
//! Buffer-specific override). Real `buf.toString()` needs `JsKind`-level
//! Buffer tracking in the front-end's Lowerer (a `resolve_method`
//! `RecvClass::Buffer` row analogous to `RecvClass::Array`'s `ARRAY_ROWS`) —
//! deferred as its own follow-up, not attempted here.
//!
//! Deferred (blocked on the same runtime object-backed-class dispatch gap):
//! the remaining Buffer INSTANCE methods (`write`/`slice`/`readUInt8`/
//! `writeUInt8`/`equals`/`fill`/`indexOf`/`copy`/…), and the `Blob`/`File`
//! classes (need blob/stream backing).
//!
//! Layout: `ops` (base64 + byte ops + `atob`/`btoa` externs), `class` (the
//! `#[rtse::class]`-backed `Buffer` statics), `mod` (registration).

mod class;
mod ops;

use rts_engine::Engine;

/// Registers the `Buffer` class (statics) + the `node:buffer` module.
pub fn register(e: &mut Engine) {
    class::register(e);
    e.module("node:buffer", |m| {
        m.doc("Buffer/base64 (node:buffer): atob, btoa.");
        m.registry(ops::atob_entry());
        m.registry(ops::btoa_entry());
    });
}
