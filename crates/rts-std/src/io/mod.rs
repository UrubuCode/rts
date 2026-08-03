//! `io` namespace — standard input/output primitives backed by `std::io`.
//!
//! `print`/`eprint`/`*_write` take a UTF-8 string (`Str`) and write its bytes;
//! `stdin_*` take a writable buffer pointer (`U64` cast to `*mut u8`) + length.
//! `*_write`/`*_flush` use `on_null = -1` to match the error convention.
//!
//! Authored with `#[rtse::function]` — the SINGLE SOURCE OF TRUTH for symbols
//! (docs/engine/architecture.md).

use std::io::{self, BufRead, Read, Write};

use rts_engine::abi::ty::{I64, U64};
use rts_engine::Engine;

/// Writes a UTF-8 message followed by newline to stdout.
#[rtse::function(module = "io", value = "print")]
pub fn print(message: &str) {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(message.as_bytes());
    let _ = lock.write_all(b"\n");
}

/// Writes a UTF-8 message followed by newline to stderr.
#[rtse::function(module = "io", value = "eprint")]
pub fn eprint(message: &str) {
    let stderr = io::stderr();
    let mut lock = stderr.lock();
    let _ = lock.write_all(message.as_bytes());
    let _ = lock.write_all(b"\n");
}

/// Writes raw bytes to stdout, returns bytes written or -1 on error.
#[rtse::function(module = "io", value = "stdout_write")]
fn stdout_write(data: &str) -> I64 {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match lock.write(data.as_bytes()) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Flushes stdout buffer. Returns 0 on success, -1 on error.
#[rtse::function(module = "io", value = "stdout_flush")]
fn stdout_flush() -> I64 {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match lock.flush() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Writes raw bytes to stderr, returns bytes written or -1 on error.
#[rtse::function(module = "io", value = "stderr_write")]
fn stderr_write(data: &str) -> I64 {
    let stderr = io::stderr();
    let mut lock = stderr.lock();
    match lock.write(data.as_bytes()) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Flushes stderr buffer. Returns 0 on success, -1 on error.
#[rtse::function(module = "io", value = "stderr_flush")]
fn stderr_flush() -> I64 {
    let stderr = io::stderr();
    let mut lock = stderr.lock();
    match lock.flush() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Reads up to `len` bytes from stdin into buffer. Returns byte count or -1.
#[rtse::function(module = "io", value = "stdin_read")]
fn stdin_read(buf_ptr: U64, buf_len: I64) -> I64 {
    if buf_ptr == 0 || buf_len <= 0 {
        return -1;
    }
    // SAFETY: caller guarantees `buf_ptr` is writable for `buf_len` bytes.
    let slot = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len as usize) };
    let stdin = io::stdin();
    let mut lock = stdin.lock();
    match lock.read(slot) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Reads a single line from stdin (no terminator) into buffer.
#[rtse::function(module = "io", value = "stdin_read_line")]
fn stdin_read_line(buf_ptr: U64, buf_len: I64) -> I64 {
    if buf_ptr == 0 || buf_len <= 0 {
        return -1;
    }
    let stdin = io::stdin();
    let mut line = String::new();
    let read = match stdin.lock().read_line(&mut line) {
        Ok(n) => n,
        Err(_) => return -1,
    };
    let bytes = line.trim_end_matches(['\r', '\n']).as_bytes();
    let copy = bytes.len().min(buf_len as usize);
    // SAFETY: caller guarantees `buf_ptr` is writable for `buf_len` bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr as *mut u8, copy);
    }
    if read == 0 { 0 } else { copy as i64 }
}

/// Registra a namespace `io` no motor.
pub fn register(e: &mut Engine) {
    e.ns("io")
        .doc("Standard input/output primitives backed by std::io.")
        .member(print_entry())
        .member(eprint_entry())
        .member(stdout_write_entry())
        .member(stdout_flush_entry())
        .member(stderr_write_entry())
        .member(stderr_flush_entry())
        .member(stdin_read_entry())
        .member(stdin_read_line_entry())
        .done();
}
