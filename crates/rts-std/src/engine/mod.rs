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
    // `Number`'s radix formatter (`rts-primitives/src/number/format.rs`) — the
    // ONE Number formatter still needed outside the value-class method dispatch:
    // `rts-adapters`'s `__rtsadp_dyn_to_string_radix` (DYNAMIC/unproven-receiver
    // `x.toString(radix)`) calls the bridge below directly.
    fn __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v: f64, radix: i64) -> u64;
}

/// `x.toString(radix)` on a DYNAMIC (unproven-type) receiver — wraps the Number
/// value-class's radix formatter. Called directly (a plain Rust fn, not a
/// Registry member) by `rts-adapters`'s `__rtsadp_dyn_to_string_radix`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX(v: f64, radix: i64) -> Handle {
    unsafe { __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v, radix) }
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
        ret_class: None,
        pure: false,
        emit: None,
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
        ret_class: None,
        pure: false,
        emit: None,
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
        .done();
}
