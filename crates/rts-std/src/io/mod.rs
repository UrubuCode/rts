//! `io` namespace — standard input/output primitives backed by `std::io`.
//!
//! `print`/`eprint`/`*_write` take a UTF-8 string (`Str`) and write its bytes;
//! `stdin_*` take a writable buffer pointer (`U64` cast to `*mut u8`) + length.
//! `*_write`/`*_flush` use `on_null = -1` to match the error convention.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::io::{self, BufRead, Read, Write};

use rts_engine::abi::ty::{I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Writes a UTF-8 message followed by newline to stdout.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_PRINT(message_ptr: *const u8, message_len: i64) {
    let message = match unsafe { rts_engine::abi::str_abi::from_abi(message_ptr, message_len) } {
        Some(s) => s,
        None => return,
    };
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(message.as_bytes());
    let _ = lock.write_all(b"\n");
}

/// Writes a UTF-8 message followed by newline to stderr.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_EPRINT(message_ptr: *const u8, message_len: i64) {
    let message = match unsafe { rts_engine::abi::str_abi::from_abi(message_ptr, message_len) } {
        Some(s) => s,
        None => return,
    };
    let stderr = io::stderr();
    let mut lock = stderr.lock();
    let _ = lock.write_all(message.as_bytes());
    let _ = lock.write_all(b"\n");
}

/// Writes raw bytes to stdout, returns bytes written or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDOUT_WRITE(data_ptr: *const u8, data_len: i64) -> I64 {
    let data = match unsafe { rts_engine::abi::str_abi::from_abi(data_ptr, data_len) } {
        Some(s) => s,
        None => return -1,
    };
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match lock.write(data.as_bytes()) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Flushes stdout buffer. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDOUT_FLUSH() -> I64 {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    match lock.flush() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Writes raw bytes to stderr, returns bytes written or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDERR_WRITE(data_ptr: *const u8, data_len: i64) -> I64 {
    let data = match unsafe { rts_engine::abi::str_abi::from_abi(data_ptr, data_len) } {
        Some(s) => s,
        None => return -1,
    };
    let stderr = io::stderr();
    let mut lock = stderr.lock();
    match lock.write(data.as_bytes()) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

/// Flushes stderr buffer. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDERR_FLUSH() -> I64 {
    let stderr = io::stderr();
    let mut lock = stderr.lock();
    match lock.flush() {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Reads up to `len` bytes from stdin into buffer. Returns byte count or -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDIN_READ(buf_ptr: U64, buf_len: I64) -> I64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_IO_STDIN_READ_LINE(buf_ptr: U64, buf_len: I64) -> I64 {
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

/// Função `io.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `io` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("io")
        .doc("Standard input/output primitives backed by std::io.")
        .member(func(
            "print",
            "__RTS_FN_NS_IO_PRINT",
            Sig::new(vec![AbiType::StrPtr], AbiType::Void),
            "print(message: string): void",
            "Writes a UTF-8 message followed by newline to stdout.",
            __RTS_FN_NS_IO_PRINT as *const u8,
        ))
        .member(func(
            "eprint",
            "__RTS_FN_NS_IO_EPRINT",
            Sig::new(vec![AbiType::StrPtr], AbiType::Void),
            "eprint(message: string): void",
            "Writes a UTF-8 message followed by newline to stderr.",
            __RTS_FN_NS_IO_EPRINT as *const u8,
        ))
        .member(func(
            "stdout_write",
            "__RTS_FN_NS_IO_STDOUT_WRITE",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "stdout_write(data: string): number",
            "Writes raw bytes to stdout, returns bytes written or -1 on error.",
            __RTS_FN_NS_IO_STDOUT_WRITE as *const u8,
        ))
        .member(func(
            "stdout_flush",
            "__RTS_FN_NS_IO_STDOUT_FLUSH",
            Sig::new(Vec::new(), AbiType::I64),
            "stdout_flush(): number",
            "Flushes stdout buffer. Returns 0 on success, -1 on error.",
            __RTS_FN_NS_IO_STDOUT_FLUSH as *const u8,
        ))
        .member(func(
            "stderr_write",
            "__RTS_FN_NS_IO_STDERR_WRITE",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "stderr_write(data: string): number",
            "Writes raw bytes to stderr, returns bytes written or -1 on error.",
            __RTS_FN_NS_IO_STDERR_WRITE as *const u8,
        ))
        .member(func(
            "stderr_flush",
            "__RTS_FN_NS_IO_STDERR_FLUSH",
            Sig::new(Vec::new(), AbiType::I64),
            "stderr_flush(): number",
            "Flushes stderr buffer. Returns 0 on success, -1 on error.",
            __RTS_FN_NS_IO_STDERR_FLUSH as *const u8,
        ))
        .member(func(
            "stdin_read",
            "__RTS_FN_NS_IO_STDIN_READ",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "stdin_read(bufPtr: number, bufLen: number): number",
            "Reads up to `len` bytes from stdin into buffer. Returns byte count or -1.",
            __RTS_FN_NS_IO_STDIN_READ as *const u8,
        ))
        .member(func(
            "stdin_read_line",
            "__RTS_FN_NS_IO_STDIN_READ_LINE",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::I64),
            "stdin_read_line(bufPtr: number, bufLen: number): number",
            "Reads a single line from stdin (no terminator) into buffer.",
            __RTS_FN_NS_IO_STDIN_READ_LINE as *const u8,
        ))
        .done();
}
