//! `process.stdout` / `process.stderr` / `process.stdin` — a narrow surface,
//! honestly.
//!
//! Node's three are full `stream.Writable`/`Readable` instances — backpressure,
//! `'drain'`, `'data'`, encodings, all of it. `node:stream` in this crate is
//! built around JavaScript-side buffers, and wiring a real file descriptor into
//! one is that module's job rather than something to reproduce here.
//!
//! So what these are is stated rather than implied: a plain object carrying
//! `.fd` and ONE method — `write` on the two writers, `read` on the reader —
//! over `std::io`'s handles. They are not `Writable`/`Readable`, they emit no
//! events, and `process.stdout.on('drain', …)` reaches nothing. What a program
//! actually blocks on, `process.stdout.write(...)`, works.

use rts_core_rwk::entry;
use std::io::{Read, Write};

/// Puts the three standard streams on the namespace.
pub(super) fn install(context: &mut entry::Context, namespace: u64) {
    let stdout = writer(context, 1.0, write_stdout);
    entry::put_member(context, namespace, "stdout", stdout);
    let stderr = writer(context, 2.0, write_stderr);
    entry::put_member(context, namespace, "stderr", stderr);

    let stdin = entry::make_object(context);
    let fd = entry::make_number(0.0);
    entry::put_member(context, stdin, "fd", fd);
    let read = entry::make_callable(context, read_stdin);
    entry::put_member(context, stdin, "read", read);
    entry::put_member(context, namespace, "stdin", stdin);
}

/// A `{ fd, write(chunk) }` object over one of the writable streams.
fn writer(context: &mut entry::Context, fd: f64, code: entry::Provided) -> u64 {
    let object = entry::make_object(context);
    let fd_value = entry::make_number(fd);
    entry::put_member(context, object, "fd", fd_value);
    let write = entry::make_callable(context, code);
    entry::put_member(context, object, "write", write);
    object
}

/// `process.stdout.write(chunk)`. Answers `true`, matching the common case of
/// Node's own return — there is no backpressure model here to report `false`
/// from, and inventing one would be a signal a program would wait on.
extern "C" fn write_stdout(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(bytes) = chunk(value) {
        let _ = std::io::stdout().write_all(&bytes);
    }
    entry::boolean_value(true)
}

/// `process.stderr.write(chunk)` — same shape, other stream.
extern "C" fn write_stderr(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(bytes) = chunk(value) {
        let _ = std::io::stderr().write_all(&bytes);
    }
    entry::boolean_value(true)
}

/// `process.stdin.read()` — everything still on the input, or `null` at EOF.
///
/// Node's `read()` answers what is currently buffered and `null` when nothing
/// is. This reads to EOF instead, because there is no reader loop here to be
/// buffering into: for the piped-input case a program actually writes, the two
/// agree after one call, and the divergence is that a second call answers
/// `null` where Node might still have had more. Stated rather than papered
/// over; `'data'`/`'end'` events do not exist here at all.
extern "C" fn read_stdin(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let mut text = String::new();
    match std::io::stdin().read_to_string(&mut text) {
        Ok(0) | Err(_) => entry::null_value(),
        Ok(_) => entry::with_runtime(|context| entry::make_string(context, &text)),
    }
}

/// A chunk to write: its BYTES when it is one of the byte views, its text when
/// it is text.
///
/// Bytes first, and this is the order that matters: `Buffer`/`Uint8Array` is
/// half of what Node's `write` accepts, and asking for text first would send a
/// buffer's `toString` shape down the stream instead of its contents.
fn chunk(value: u64) -> Option<Vec<u8>> {
    let absent = entry::undefined_value();
    if value == absent {
        return None;
    }
    if let Some(bytes) = entry::with_runtime(|context| entry::bytes_of(context, value)) {
        return Some(bytes);
    }
    entry::text_of(value).map(String::into_bytes)
}
