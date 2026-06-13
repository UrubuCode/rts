//! `console.log` capture — the runtime sink the JIT'd program prints into.
//!
//! The harness ([`crate::front::run`]) drives a whole `.ts` program to native
//! code and needs to read back what it printed. Rather than write to the real
//! stdout (untestable, racy), `console.log` appends to a process-global capture
//! buffer; the host drains it with [`take_output`] after the run.
//!
//! ## Why fixed-arity entries
//!
//! `console.log` is variadic. Marshaling a variadic call through the JIT means
//! either a C varargs ABI (fragile across targets) or a caller-built stack array
//! (a slot-array the lowering fills, easy to get subtly wrong). Instead we expose
//! a small family of fixed-arity extern entries — [`__rtsn_console_log0`] …
//! [`__rtsn_console_log6`] — and the lowering picks the one matching the call's
//! argument count. Each takes raw `u64` PolyValue words, `ToString`s them, joins
//! with a single space, and pushes the line + `"\n"`. Calls with more than 6
//! arguments are an explicit `Unsupported` bail in the lowering (never silently
//! truncated).

use std::sync::{Mutex, OnceLock};

use crate::runtime::tostring::js_to_string;
use crate::value::PolyValue;

/// The process-global capture buffer. `console.log` appends; the host drains.
fn buffer() -> &'static Mutex<String> {
    static BUF: OnceLock<Mutex<String>> = OnceLock::new();
    BUF.get_or_init(|| Mutex::new(String::new()))
}

/// A process-global lock making `reset_output → run → take_output` atomic for a
/// single program run. The capture buffer is shared across the whole process, so
/// two programs run concurrently (e.g. parallel tests) would interleave their
/// output. The harness holds [`run_lock`] for the duration of one run so each
/// run's captured stdout is exactly its own.
pub fn run_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Append one `console.log` line: the already-`ToString`'d args joined by a
/// single space, followed by `"\n"`.
fn push_line(parts: &[String]) {
    let line = parts.join(" ");
    let mut guard = buffer().lock().expect("console buffer poisoned");
    guard.push_str(&line);
    guard.push('\n');
}

/// Drain and return everything `console.log` has captured so far (host-only,
/// not part of the JIT ABI). Leaves the buffer empty.
pub fn take_output() -> String {
    let mut guard = buffer().lock().expect("console buffer poisoned");
    std::mem::take(&mut *guard)
}

/// Clear the capture buffer (host-only). Call before a run so stale output from
/// a previous program does not leak in.
pub fn reset_output() {
    buffer().lock().expect("console buffer poisoned").clear();
}

/// `ToString` a raw PolyValue word for the log line.
#[inline]
fn s(arg: u64) -> String {
    js_to_string(PolyValue::from_raw(arg))
}

// ---------------------------------------------------------------------------
// Fixed-arity extern entries. Each prints `argc` PolyValue args joined by space.
// ---------------------------------------------------------------------------

/// `console.log()` — prints a blank line.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_console_log0() {
    push_line(&[]);
}

/// `console.log(a)`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_console_log1(a: u64) {
    push_line(&[s(a)]);
}

/// `console.log(a, b)`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_console_log2(a: u64, b: u64) {
    push_line(&[s(a), s(b)]);
}

/// `console.log(a, b, c)`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_console_log3(a: u64, b: u64, c: u64) {
    push_line(&[s(a), s(b), s(c)]);
}

/// `console.log(a, b, c, d)`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_console_log4(a: u64, b: u64, c: u64, d: u64) {
    push_line(&[s(a), s(b), s(c), s(d)]);
}

/// `console.log(a, b, c, d, e)`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_console_log5(a: u64, b: u64, c: u64, d: u64, e: u64) {
    push_line(&[s(a), s(b), s(c), s(d), s(e)]);
}

/// `console.log(a, b, c, d, e, f)`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsn_console_log6(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) {
    push_line(&[s(a), s(b), s(c), s(d), s(e), s(f)]);
}

/// The fixed-arity `console.log` entry name for `argc` args, or `None` if `argc`
/// exceeds the supported maximum (6). The lowering uses this to pick the symbol
/// and to bail explicitly above the cap.
pub fn console_log_symbol(argc: usize) -> Option<&'static str> {
    Some(match argc {
        0 => "__rtsn_console_log0",
        1 => "__rtsn_console_log1",
        2 => "__rtsn_console_log2",
        3 => "__rtsn_console_log3",
        4 => "__rtsn_console_log4",
        5 => "__rtsn_console_log5",
        6 => "__rtsn_console_log6",
        _ => return None,
    })
}
