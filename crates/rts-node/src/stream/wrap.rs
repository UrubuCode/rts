//! `readable.wrap(oldStream)` — the streams-v1 adapter: turns an
//! `EventEmitter` that emits `'data'`/`'end'`/`'error'`/`'close'` and answers
//! `.pause()`/`.resume()` into a real `Readable`.
//!
//! # Mechanism
//!
//! `this` is what every listener and `_read` needs, and a closure here can
//! only capture ONE value ([`entry::closure_new`]'s own limit — see
//! `writable.rs`'s module doc for the same wall). So `this` is captured, not
//! `oldStream`, and every hook reads `oldStream` back off `this.__wrapped__`
//! — a plain own property, [`common`]'s usual "state lives on the instance"
//! convention rather than a second table.
//!
//! # Not implemented, by name
//!
//! Node also copies every enumerable method `oldStream` carries that `this`
//! does not already have, so a legacy stream's own extra methods stay
//! reachable through the wrapper. That copy is not done here: only the five
//! events (`data`/`end`/`error`/`close`/`destroy`) and the `pause`/`resume`
//! pair `_read` needs are wired. A program calling a domain-specific method of
//! the wrapped object through the WRAPPER (rather than through the object it
//! already holds) finds it absent.

use rts_core::entry::{self, Provided};

use super::common::*;

const WRAPPED: &str = "__wrapped__";

const FORWARDED: &[(&str, Provided)] =
    &[("data", on_data), ("end", on_end), ("error", on_error), ("close", on_close), ("destroy", on_destroy)];

/// `readable.wrap(oldStream)`.
pub(super) extern "C" fn wrap(_e: u64, this: u64, old_stream: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    entry::with_runtime(|context| set_value(context, this, WRAPPED, old_stream));
    let on_fn = entry::with_runtime(|context| entry::get_member(context, old_stream, "on"));
    if on_fn != absent {
        for (event, hook) in FORWARDED {
            let listener = entry::closure_new(*hook as *const () as usize as i64, this);
            entry::call(on_fn, old_stream, key(event), listener, absent, absent);
        }
    }
    let read_hook = entry::closure_new(read as *const () as usize as i64, this);
    entry::with_runtime(|context| set_value(context, this, "_read", read_hook));
    this
}

/// `this._read` — resumes the legacy source, which is how a pre-streams2
/// object is asked to produce: `.pause()`/`.resume()` are its own flow
/// control, and there is no `_read(size)` protocol on the other side of this
/// adapter to ask more precisely.
extern "C" fn read(this: u64, _t: u64, _size: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let old_stream = get_value(this, WRAPPED);
    let resume_fn = entry::with_runtime(|context| entry::get_member(context, old_stream, "resume"));
    let absent = entry::undefined_value();
    if resume_fn != absent {
        entry::call(resume_fn, old_stream, absent, absent, absent, absent);
    }
    absent
}

/// `oldStream.on('data', …)` — pauses the source when `this` reports
/// backpressure, the same signal `readable::push`'s own caller already reads.
extern "C" fn on_data(this: u64, _t: u64, chunk: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let ok = super::readable::push(0, this, chunk, absent, 0, 0);
    if ok == entry::boolean_value(false) {
        let old_stream = get_value(this, WRAPPED);
        let pause_fn = entry::with_runtime(|context| entry::get_member(context, old_stream, "pause"));
        if pause_fn != absent {
            entry::call(pause_fn, old_stream, absent, absent, absent, absent);
        }
    }
    absent
}

extern "C" fn on_end(this: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::readable::push(0, this, entry::null_value(), entry::undefined_value(), 0, 0);
    entry::undefined_value()
}

extern "C" fn on_error(this: u64, _t: u64, error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::readable::destroy(0, this, error, 0, 0, 0);
    entry::undefined_value()
}

extern "C" fn on_close(this: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    emit(this, "close", absent, absent, absent);
    absent
}

extern "C" fn on_destroy(this: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    super::readable::destroy(0, this, entry::undefined_value(), 0, 0, 0);
    entry::undefined_value()
}
