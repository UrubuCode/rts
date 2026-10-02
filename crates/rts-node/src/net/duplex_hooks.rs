//! The three Duplex hooks a `net.Socket` installs on itself — `_write`,
//! `_final` and `_destroy`.
//!
//! One module because they are the whole of what `stream`'s writable protocol
//! calls into for a socket, and each one is the OS half of a JS-level act:
//! `write()` sends bytes, `end()` sends FIN, `destroy()` closes the descriptor.
//! The JS-level bookkeeping around all three belongs to `stream` and is
//! inherited, never restated here.
//!
//! They were in `socket.rs` until `_final` was written, which took that file
//! past the 500-line ceiling. Moving the three out rather than only the new one:
//! a file that holds two of a set of three is the reason the third gets written
//! somewhere else next time.

use std::net::Shutdown;

use rts_core::entry;

use super::registry;

/// Installs all three on `instance`. Called from `socket::init`.
pub(super) fn install(context: &mut entry::Context, instance: u64) {
    for (name, hook) in [
        ("_write", write_hook as extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64),
        ("_final", final_hook),
        ("_destroy", destroy_hook),
    ] {
        let value = entry::make_callable(context, hook);
        super::common::set_value(context, instance, name, value);
    }
}

/// Installed as `this._write` — see `socket.rs`'s module doc.
extern "C" fn write_hook(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    registry::pump();
    let absent = entry::undefined_value();
    let bytes = entry::text_of(chunk)
        .and_then(|text| entry::encode_text(&text, "utf8"))
        .or_else(|| entry::with_runtime(|context| entry::bytes_of(context, chunk)))
        .unwrap_or_default();
    let Some(id) = super::socket::socket_id(this) else {
        entry::call(callback, absent, absent, absent, absent, absent);
        return absent;
    };
    match registry::write_now(id, &bytes) {
        Ok(()) => {
            let written = super::common::get_num(this, "bytesWritten") + bytes.len() as f64;
            entry::with_runtime(|context| super::common::set_num(context, this, "bytesWritten", written));
            entry::call(callback, absent, absent, absent, absent, absent);
        }
        Err(error) => {
            let text = error.to_string();
            let message = entry::with_runtime(|context| entry::make_string(context, &text));
            entry::call(callback, absent, message, absent, absent, absent);
        }
    }
    absent
}

/// Installed as `this._final` — sends the FIN that `socket.end()` means.
///
/// Only `_write` and `_destroy` were installed, so `end()` ran the whole
/// writable protocol (`'finish'`, the `end` callback) and left the OS socket
/// fully open. The PEER therefore never saw EOF, so it never emitted `'end'` or
/// `'close'` either, and an echo exchange where each side ends after the other's
/// reply hung with both sides reading. Node's `end()` shuts the write half,
/// which is what makes a half-close observable at all.
///
/// `Shutdown::Write` and not `Both`: the readable half is still live after
/// `end()` — a server that ends its response still reads a client that has not —
/// and closing it here would turn a half-close into a reset.
extern "C" fn final_hook(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = super::socket::socket_id(this) {
        registry::with_sockets(|table| {
            if let Some(entry) = table.get(&id)
                && let Some(stream) = &entry.stream
            {
                let _ = stream.shutdown(Shutdown::Write);
            }
        });
    }
    let absent = entry::undefined_value();
    // The writable protocol is waiting on this callback — `maybe_finish` pushed
    // onto its PENDING stack before the call, and leaving it unpopped would
    // mis-attribute the NEXT stream's `_write` completion.
    if callback != absent {
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    absent
}

/// Installed as `this._destroy` — closes the OS socket; the JS-side
/// bookkeeping (`destroyed`, `'close'`) is `readable::destroy`'s, inherited.
extern "C" fn destroy_hook(_e: u64, this: u64, _error: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = super::socket::socket_id(this) {
        registry::with_sockets(|table| {
            if let Some(entry) = table.get_mut(&id) {
                entry.closed = true;
                if let Some(stream) = &entry.stream {
                    let _ = stream.shutdown(Shutdown::Both);
                }
            }
        });
    }
    entry::undefined_value()
}
