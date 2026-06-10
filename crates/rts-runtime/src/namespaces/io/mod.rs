//! `io` namespace — standard input/output primitives backed by `std::io`.
//!
//! `print`/`eprint`/`*_write` take a UTF-8 string (`Str`) and write its bytes;
//! `stdin_*` take a writable buffer pointer (`U64` cast to `*mut u8`) + length.
//! `*_write`/`*_flush` use `on_null = -1` to match the error convention.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::io::{self, BufRead, Read, Write};

use rts_engine::abi::ty::{I64, U64};
use rts_macro::rts_namespace;

/// Standard input/output primitives backed by std::io.
#[rts_namespace(io)]
impl IoNs {
    /// Writes a UTF-8 message followed by newline to stdout.
    #[rts_fn(ts = "print(message: string): void")]
    pub fn print(message: Str) {
        let stdout = io::stdout();
        let mut lock = stdout.lock();
        let _ = lock.write_all(message.as_bytes());
        let _ = lock.write_all(b"\n");
    }

    /// Writes a UTF-8 message followed by newline to stderr.
    #[rts_fn(ts = "eprint(message: string): void")]
    pub fn eprint(message: Str) {
        let stderr = io::stderr();
        let mut lock = stderr.lock();
        let _ = lock.write_all(message.as_bytes());
        let _ = lock.write_all(b"\n");
    }

    /// Writes raw bytes to stdout, returns bytes written or -1 on error.
    #[rts_fn(ts = "stdout_write(data: string): number", on_null = -1)]
    pub fn stdout_write(data: Str) -> I64 {
        let stdout = io::stdout();
        let mut lock = stdout.lock();
        match lock.write(data.as_bytes()) {
            Ok(n) => n as i64,
            Err(_) => -1,
        }
    }

    /// Flushes stdout buffer. Returns 0 on success, -1 on error.
    #[rts_fn]
    pub fn stdout_flush() -> I64 {
        let stdout = io::stdout();
        let mut lock = stdout.lock();
        match lock.flush() {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    /// Writes raw bytes to stderr, returns bytes written or -1 on error.
    #[rts_fn(ts = "stderr_write(data: string): number", on_null = -1)]
    pub fn stderr_write(data: Str) -> I64 {
        let stderr = io::stderr();
        let mut lock = stderr.lock();
        match lock.write(data.as_bytes()) {
            Ok(n) => n as i64,
            Err(_) => -1,
        }
    }

    /// Flushes stderr buffer. Returns 0 on success, -1 on error.
    #[rts_fn]
    pub fn stderr_flush() -> I64 {
        let stderr = io::stderr();
        let mut lock = stderr.lock();
        match lock.flush() {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    /// Reads up to `len` bytes from stdin into buffer. Returns byte count or -1.
    #[rts_fn(ts = "stdin_read(bufPtr: number, bufLen: number): number")]
    pub fn stdin_read(buf_ptr: U64, buf_len: I64) -> I64 {
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
    #[rts_fn(ts = "stdin_read_line(bufPtr: number, bufLen: number): number")]
    pub fn stdin_read_line(buf_ptr: U64, buf_len: I64) -> I64 {
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
}
