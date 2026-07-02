//! `engine` namespace — PRIVATE engine-internal surface.
//!
//! This namespace re-exposes already-existing runtime functionality (arch from
//! `os`, timestamps from `time`, the trace frame-stack used for error stacks) to
//! the engine's OWN embedded TS prelude/includes (future `Error.ts` / `Date.ts`
//! and friends) — NOT to end-user `.ts` code. It implements NOTHING new: every
//! member WRAPS an existing `__RTS_FN_*` extern (calling it, never reimplementing
//! the logic), so there is one source of truth per capability.
//!
//! Marked `.private()` on the builder; the new engine enforces the privacy gate at
//! lowering time (the `engine` ambient global is resolvable only from
//! prelude-origin functions — see `rts-codegen-new`'s `engineobj` lowering).
//!
//! Symbols follow the canonical convention `__RTS_FN_NS_ENGINE_<NAME>`.

use rts_engine::abi::ty::Handle;
use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

// The existing externs we wrap (by symbol — their real bodies live in the
// `os` / `time` / `trace` namespaces, linked into the same runtime).
unsafe extern "C" {
    fn __RTS_FN_NS_OS_ARCH() -> u64;
    fn __RTS_FN_NS_TIME_NOW_MS() -> i64;
    fn __RTS_FN_NS_TIME_NOW_NS() -> i64;
    fn __RTS_FN_NS_TIME_UNIX_MS() -> i64;
    fn __RTS_FN_NS_TIME_UNIX_NS() -> i64;
    fn __RTS_FN_NS_TRACE_PUSH_FRAME(
        file_ptr: *const u8,
        file_len: i64,
        fn_name_ptr: *const u8,
        fn_name_len: i64,
        line: i64,
        col: i64,
    );
    fn __RTS_FN_NS_TRACE_POP_FRAME();
    fn __RTS_FN_NS_TRACE_CAPTURE() -> u64;
    fn __RTS_FN_NS_TRACE_PRINT();
    // Number formatters (rts-primitives number.rs) — the irreducible numeric
    // FORMATTING (float→string, radix, toFixed/toPrecision/toExponential). The
    // `engine.num_*` members below WRAP these so the `.ts` `class Number` method
    // bodies call the SAME Rust formatters (one source of truth) instead of
    // reimplementing them.
    fn __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v: f64, radix: i64) -> u64;
    fn __RTS_FN_GL_NUMBER_TO_FIXED(v: f64, digits: i64) -> u64;
    fn __RTS_FN_GL_NUMBER_TO_PRECISION(v: f64, digits: i64) -> u64;
    fn __RTS_FN_GL_NUMBER_TO_EXPONENTIAL(v: f64, digits: i64) -> u64;
    fn __RTS_FN_GL_NUMBER_FROM_STR(handle: u64) -> f64;
}

// The String-method bridge (`engine.str_*`) lives in its own submodule — it wraps
// the 21 `__RTS_FN_GL_STRING_*` Rust impls so the `.ts` `class String` is one
// source of truth. Split out to keep this file under the size budget. Re-exported
// so the `__RTS_FN_NS_ENGINE_STR_*` externs stay reachable by path (the JIT symbol
// table in `rts-codegen-new::runtime_link` takes their address through the facade).
mod string;
pub use string::*;

/// `n.toString(radix)` — number→string in base `radix` (2..36; 10 is plain
/// decimal). Wraps the Rust radix formatter. The `.ts` `Number.toString` body
/// calls this; default radix (10) is applied by the `.ts` default-param.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX(v: f64, radix: i64) -> Handle {
    unsafe { __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v, radix) }
}

/// `n.toFixed(digits)` — fixed-point string. Wraps the Rust formatter.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_NUM_TO_FIXED(v: f64, digits: i64) -> Handle {
    unsafe { __RTS_FN_GL_NUMBER_TO_FIXED(v, digits) }
}

/// `n.toPrecision(digits)` — significant-digits string (digits <= 0 ⇒ plain
/// toString). Wraps the Rust formatter.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_NUM_TO_PRECISION(v: f64, digits: i64) -> Handle {
    unsafe { __RTS_FN_GL_NUMBER_TO_PRECISION(v, digits) }
}

/// `n.toExponential(digits)` — exponential-notation string (digits < 0 ⇒ auto
/// mantissa). Wraps the Rust formatter.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_NUM_TO_EXPONENTIAL(v: f64, digits: i64) -> Handle {
    unsafe { __RTS_FN_GL_NUMBER_TO_EXPONENTIAL(v, digits) }
}

/// `Number(str)` — parse a string handle to f64 (NaN on failure; "" ⇒ 0). Wraps
/// the Rust parser (one source of truth). The `.ts` `NumberFactory` calls this
/// for the string case of ToNumber.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_NUM_FROM_STR(handle: Handle) -> f64 {
    unsafe { __RTS_FN_GL_NUMBER_FROM_STR(handle) }
}

/// CPU architecture string handle ('x86_64', 'aarch64', …). Wraps `os.arch`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_ARCH() -> Handle {
    unsafe { __RTS_FN_NS_OS_ARCH() }
}

/// Monotonic milliseconds since process start. Wraps `time.now_ms`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_NOW_MS() -> i64 {
    unsafe { __RTS_FN_NS_TIME_NOW_MS() }
}

/// Monotonic nanoseconds since process start. Wraps `time.now_ns`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_NOW_NS() -> i64 {
    unsafe { __RTS_FN_NS_TIME_NOW_NS() }
}

/// Wall-clock milliseconds since the UNIX epoch. Wraps `time.unix_ms`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_UNIX_MS() -> i64 {
    unsafe { __RTS_FN_NS_TIME_UNIX_MS() }
}

/// Wall-clock nanoseconds since the UNIX epoch. Wraps `time.unix_ns`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_UNIX_NS() -> i64 {
    unsafe { __RTS_FN_NS_TIME_UNIX_NS() }
}

/// Push a TS call frame onto the trace stack (for error stacks). Wraps
/// `trace.push_frame`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_TRACE_PUSH(
    file_ptr: *const u8,
    file_len: i64,
    fn_name_ptr: *const u8,
    fn_name_len: i64,
    line: i64,
    col: i64,
) {
    unsafe { __RTS_FN_NS_TRACE_PUSH_FRAME(file_ptr, file_len, fn_name_ptr, fn_name_len, line, col) }
}

/// Pop the top TS call frame from the trace stack. Wraps `trace.pop_frame`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_TRACE_POP() {
    unsafe { __RTS_FN_NS_TRACE_POP_FRAME() }
}

/// Capture the current trace as a GC string handle (0 if the stack is empty).
/// This is the renderer Error/throw stacks need: it both captures the current
/// frame stack AND renders it to a string in one step. Wraps `trace.capture`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_TRACE_CAPTURE() -> Handle {
    unsafe { __RTS_FN_NS_TRACE_CAPTURE() }
}

/// Print the current trace stack to stderr. Wraps `trace.print`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_TRACE_PRINT() {
    unsafe { __RTS_FN_NS_TRACE_PRINT() }
}

/// A member returning a GC string handle.
fn str_func(name: &str, symbol: &str, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: rts_engine::Sig::new(Vec::new(), rts_engine::AbiType::Handle),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// A generic member with an explicit signature.
pub(super) fn func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
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
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace PRIVADA `engine` no motor. Re-expõe arch/time/trace para o
/// prelude embutido do motor; marcada `.private()` para não vazar pro código do
/// usuário.
pub fn register(e: &mut Engine) {
    let b = e
        .ns("engine")
        .doc("PRIVATE engine-internal surface: arch + timestamps + trace, for the embedded TS prelude only.")
        .private()
        .member(str_func(
            "arch",
            "__RTS_FN_NS_ENGINE_ARCH",
            "arch(): string",
            "CPU architecture string ('x86_64', 'aarch64', ...). Wraps os.arch.",
            __RTS_FN_NS_ENGINE_ARCH as *const u8,
        ))
        .member(func(
            "is_buffer",
            "__RTS_FN_NS_ENGINE_IS_BUFFER",
            rts_engine::Sig::new(vec![rts_engine::AbiType::PolyValue], rts_engine::AbiType::PolyValue),
            "is_buffer(x: unknown): boolean",
            "Whether the value wraps an Entry::Buffer (an ArrayBuffer) — the structuredClone transfer bridge.",
            rts_shared::buffer::__RTS_FN_NS_ENGINE_IS_BUFFER as *const u8,
        ))
        .member(func(
            "buffer_clone",
            "__RTS_FN_NS_ENGINE_BUFFER_CLONE",
            rts_engine::Sig::new(vec![rts_engine::AbiType::PolyValue], rts_engine::AbiType::PolyValue),
            "buffer_clone(x: unknown): unknown",
            "A NEW ArrayBuffer copying the bytes (structuredClone of a buffer).",
            rts_shared::buffer::__RTS_FN_NS_ENGINE_BUFFER_CLONE as *const u8,
        ))
        .member(func(
            "buffer_detach",
            "__RTS_FN_NS_ENGINE_BUFFER_DETACH",
            rts_engine::Sig::new(vec![rts_engine::AbiType::PolyValue], rts_engine::AbiType::PolyValue),
            "buffer_detach(x: unknown): void",
            "Empty the buffer in place (JS detach: byteLength reads 0 afterwards).",
            rts_shared::buffer::__RTS_FN_NS_ENGINE_BUFFER_DETACH as *const u8,
        ))
        .member(func(
            "now_ms",
            "__RTS_FN_NS_ENGINE_NOW_MS",
            sig!(=> I64),
            "now_ms(): number",
            "Monotonic milliseconds since process start. Wraps time.now_ms.",
            __RTS_FN_NS_ENGINE_NOW_MS as *const u8,
        ))
        .member(func(
            "now_ns",
            "__RTS_FN_NS_ENGINE_NOW_NS",
            sig!(=> I64),
            "now_ns(): number",
            "Monotonic nanoseconds since process start. Wraps time.now_ns.",
            __RTS_FN_NS_ENGINE_NOW_NS as *const u8,
        ))
        .member(func(
            "unix_ms",
            "__RTS_FN_NS_ENGINE_UNIX_MS",
            sig!(=> I64),
            "unix_ms(): number",
            "Wall-clock milliseconds since the UNIX epoch. Wraps time.unix_ms.",
            __RTS_FN_NS_ENGINE_UNIX_MS as *const u8,
        ))
        .member(func(
            "unix_ns",
            "__RTS_FN_NS_ENGINE_UNIX_NS",
            sig!(=> I64),
            "unix_ns(): number",
            "Wall-clock nanoseconds since the UNIX epoch. Wraps time.unix_ns.",
            __RTS_FN_NS_ENGINE_UNIX_NS as *const u8,
        ))
        .member(func(
            "trace_push",
            "__RTS_FN_NS_ENGINE_TRACE_PUSH",
            sig!(StrPtr, StrPtr, I64, I64 => Void),
            "trace_push(file: string, fn_name: string, line: number, col: number): void",
            "Push a TS call frame onto the trace stack. Wraps trace.push_frame.",
            __RTS_FN_NS_ENGINE_TRACE_PUSH as *const u8,
        ))
        .member(func(
            "trace_pop",
            "__RTS_FN_NS_ENGINE_TRACE_POP",
            sig!(=> Void),
            "trace_pop(): void",
            "Pop the top TS call frame from the trace stack. Wraps trace.pop_frame.",
            __RTS_FN_NS_ENGINE_TRACE_POP as *const u8,
        ))
        .member(str_func(
            "trace_capture",
            "__RTS_FN_NS_ENGINE_TRACE_CAPTURE",
            "trace_capture(): string",
            "Capture + render the current trace stack to a string handle. Wraps trace.capture.",
            __RTS_FN_NS_ENGINE_TRACE_CAPTURE as *const u8,
        ))
        .member(func(
            "trace_print",
            "__RTS_FN_NS_ENGINE_TRACE_PRINT",
            sig!(=> Void),
            "trace_print(): void",
            "Print the current trace stack to stderr. Wraps trace.print.",
            __RTS_FN_NS_ENGINE_TRACE_PRINT as *const u8,
        ))
        // ── Number formatters (the irreducible numeric FORMATTING bridge). Each
        // wraps the existing Rust `__RTS_FN_GL_NUMBER_*` formatter so the `.ts`
        // `class Number` method bodies call them (one source of truth). Shape:
        // (n: number, arg: number) => string.
        .member(func(
            "num_to_string_radix",
            "__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX",
            sig!(F64, I64 => Handle),
            "num_to_string_radix(n: number, radix: number): string",
            "n.toString(radix) — number→string in base radix. Wraps GL_NUMBER_TO_STRING_RADIX.",
            __RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX as *const u8,
        ))
        .member(func(
            "num_to_fixed",
            "__RTS_FN_NS_ENGINE_NUM_TO_FIXED",
            sig!(F64, I64 => Handle),
            "num_to_fixed(n: number, digits: number): string",
            "n.toFixed(digits) — fixed-point string. Wraps GL_NUMBER_TO_FIXED.",
            __RTS_FN_NS_ENGINE_NUM_TO_FIXED as *const u8,
        ))
        .member(func(
            "num_to_precision",
            "__RTS_FN_NS_ENGINE_NUM_TO_PRECISION",
            sig!(F64, I64 => Handle),
            "num_to_precision(n: number, digits: number): string",
            "n.toPrecision(digits) — significant-digits string. Wraps GL_NUMBER_TO_PRECISION.",
            __RTS_FN_NS_ENGINE_NUM_TO_PRECISION as *const u8,
        ))
        .member(func(
            "num_to_exponential",
            "__RTS_FN_NS_ENGINE_NUM_TO_EXPONENTIAL",
            sig!(F64, I64 => Handle),
            "num_to_exponential(n: number, digits: number): string",
            "n.toExponential(digits) — exponential-notation string. Wraps GL_NUMBER_TO_EXPONENTIAL.",
            __RTS_FN_NS_ENGINE_NUM_TO_EXPONENTIAL as *const u8,
        ))
        .member(func(
            "num_from_str",
            "__RTS_FN_NS_ENGINE_NUM_FROM_STR",
            sig!(Handle => F64),
            "num_from_str(s: string): number",
            "Number(str) — string→number parse (NaN on failure). Wraps GL_NUMBER_FROM_STR.",
            __RTS_FN_NS_ENGINE_NUM_FROM_STR as *const u8,
        ));
    // ── String method bridge (the irreducible Unicode-aware string logic). The
    // 21 `engine.str_*` members live in the `string` submodule (each wraps a
    // `__RTS_FN_GL_STRING_*` impl — one source of truth); add them to the builder.
    string::register_members(b).done();
}
