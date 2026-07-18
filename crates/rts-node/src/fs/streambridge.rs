//! node:fs — the PRIVATE file-IO bridge for the `.ts` `ReadStream`/`WriteStream`
//! prelude (`fs/stream.ts`). Those classes extend the ambient `stream.Readable`/
//! `stream.Writable` (so `.pipe`/`instanceof`/backpressure come for free); the one
//! thing a `.ts` prelude cannot do is touch the filesystem, so these three
//! `engine.*` bridges expose exactly that — read all bytes, write (truncate), and
//! append — over real `std::fs`.
//!
//! Each takes/returns a raw `PolyValue` WORD (the `engine.*` lowering passes words
//! verbatim; see `engineobj::lower_engine_descriptor`): the path is a boxed string
//! word, the data a boxed `Uint8Array`/`Buffer`/string word, and the read result a
//! boxed `Uint8Array` word. Failures throw a Node-style error through the JS error
//! slot (`throw_io`) — the `.ts` wraps the call in try/catch and re-emits it as the
//! stream's `'error'` event, exactly as Node does.
//!
//! Registered as members of a PRIVATE namespace (`fs/mod.rs`) so their symbols
//! harvest into the JIT table, but user code cannot import them; the `engine.*`
//! privacy gate additionally restricts the callers to prelude-origin code.

use rts_engine::heap::shapes::handle_word_auto;

use super::words::{byte_array, read_bytes, throw_io};
use crate::values::{val, Val};

/// Decode a boxed string word to its UTF-8 path; empty string for a non-string
/// (the `.ts` only ever passes a string path, so this is a defensive default).
fn path_of(word: u64) -> String {
    match val(word) {
        Val::Str(s) => s,
        _ => String::new(),
    }
}

/// The raw bytes behind a data word (`Uint8Array`/`Buffer`/`ArrayBuffer`/string).
fn bytes_of(word: u64) -> Vec<u8> {
    match val(word) {
        Val::Str(s) => s.into_bytes(),
        Val::Obj(handle) => read_bytes(handle),
        _ => Vec::new(),
    }
}

/// `engine.fs_read_bytes(path)` → a `Uint8Array` word of the whole file. Throws a
/// Node-style error (ENOENT/EACCES/…) on failure.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_FS_READ_BYTES(path_word: u64) -> u64 {
    let path = path_of(path_word);
    match std::fs::read(&path) {
        Ok(bytes) => handle_word_auto(byte_array(&bytes)),
        Err(e) => {
            throw_io(&e, "open", &path);
            handle_word_auto(byte_array(&[]))
        }
    }
}

/// `engine.fs_write_bytes(path, data)` — truncate-write; returns the byte count
/// written as a number word. Throws on failure.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_FS_WRITE_BYTES(path_word: u64, data_word: u64) -> u64 {
    let path = path_of(path_word);
    let bytes = bytes_of(data_word);
    match std::fs::write(&path, &bytes) {
        Ok(()) => f64::from(bytes.len() as u32).to_bits(),
        Err(e) => {
            throw_io(&e, "open", &path);
            f64::from(0u32).to_bits()
        }
    }
}

/// `engine.fs_append_bytes(path, data)` — append (creating the file if missing);
/// returns the byte count written. Throws on failure.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_FS_APPEND_BYTES(path_word: u64, data_word: u64) -> u64 {
    use std::io::Write;
    let path = path_of(path_word);
    let bytes = bytes_of(data_word);
    let r = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(&bytes));
    match r {
        Ok(()) => f64::from(bytes.len() as u32).to_bits(),
        Err(e) => {
            throw_io(&e, "open", &path);
            f64::from(0u32).to_bits()
        }
    }
}
