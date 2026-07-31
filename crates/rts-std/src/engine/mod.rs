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
//! Symbols keep the LEGACY `__RTS_FN_NS_ENGINE_<NAME>` spelling (via an
//! EXPLICIT `#[rtse::function("__RTS_FN_NS_ENGINE_...")]` symbol, not the
//! macro's derived `__rtsm_engine_<name>`): `rts-codegen-new`'s
//! `front/run/engineobj.rs` lowers every `engine.method(...)` call by
//! hardcoding these exact symbol strings (`engine_member`/`engine_word_member`)
//! and calling `call_runtime` directly — it bypasses the Registry lookup
//! entirely, so renaming the symbol here without updating that hardcoded table
//! (out of scope for this conversion) would silently break every prelude
//! caller. `is_buffer`/`buffer_clone`/`buffer_detach` additionally reuse an
//! existing `rts-shared` extern under its own symbol (see the `func` escape
//! hatch below); `num_to_string_radix` is a plain hand-written bridge fn.

use rts_engine::abi::ty::Handle;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

// The existing externs we wrap (by symbol — their real bodies live in the
// `os` / `time` / `trace` namespaces, linked into the same runtime).
unsafe extern "C" {
    fn __rtsm_os_arch() -> u64;
    fn __rtsm_time_now_ms() -> i64;
    fn __rtsm_time_now_ns() -> i64;
    fn __rtsm_time_unix_ms() -> i64;
    fn __rtsm_time_unix_ns() -> i64;
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
    // `rts-runtime`'s `__rtsadp_dyn_to_string_radix` (DYNAMIC/unproven-receiver
    // `x.toString(radix)`) calls the bridge below directly.
    fn __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v: f64, radix: i64) -> u64;
}

/// `x.toString(radix)` on a DYNAMIC (unproven-type) receiver — wraps the Number
/// value-class's radix formatter. Called directly (a plain Rust fn, not a
/// Registry member) by `rts-runtime`'s `__rtsadp_dyn_to_string_radix`.
#[rtse::abi("__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX")]
pub fn __RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX(v: f64, radix: i64) -> Handle {
    unsafe { __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v, radix) }
}

/// CPU architecture string handle ('x86_64', 'aarch64', …). Wraps `os.arch`.
#[rtse::function("__RTS_FN_NS_ENGINE_ARCH")]
fn arch() -> Handle {
    unsafe { __rtsm_os_arch() }
}

/// A generic member with an explicit signature — the escape hatch for the 3
/// members below, which do not have a Rust BODY of their own: they reuse an
/// EXISTING `#[unsafe(no_mangle)]` extern already defined in `rts-shared`
/// (`buffer/mod.rs`), under that exact same symbol. `#[rtse::function]` cannot
/// express this: it always MINTS a fresh symbol from `module`+`value`, and
/// here that would collide with the symbol `rts-shared` already owns.
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

/// Monotonic milliseconds since process start. Wraps `time.now_ms`.
#[rtse::function("__RTS_FN_NS_ENGINE_NOW_MS")]
fn now_ms() -> i64 {
    unsafe { __rtsm_time_now_ms() }
}

/// Monotonic nanoseconds since process start. Wraps `time.now_ns`.
#[rtse::function("__RTS_FN_NS_ENGINE_NOW_NS")]
fn now_ns() -> i64 {
    unsafe { __rtsm_time_now_ns() }
}

/// Wall-clock milliseconds since the UNIX epoch. Wraps `time.unix_ms`.
#[rtse::function("__RTS_FN_NS_ENGINE_UNIX_MS")]
fn unix_ms() -> i64 {
    unsafe { __rtsm_time_unix_ms() }
}

/// Wall-clock nanoseconds since the UNIX epoch. Wraps `time.unix_ns`.
#[rtse::function("__RTS_FN_NS_ENGINE_UNIX_NS")]
fn unix_ns() -> i64 {
    unsafe { __rtsm_time_unix_ns() }
}

/// Push a TS call frame onto the trace stack. Wraps trace.push_frame.
#[rtse::function("__RTS_FN_NS_ENGINE_TRACE_PUSH")]
fn trace_push(file: &str, fn_name: &str, line: i64, col: i64) {
    unsafe {
        __RTS_FN_NS_TRACE_PUSH_FRAME(
            file.as_ptr(),
            file.len() as i64,
            fn_name.as_ptr(),
            fn_name.len() as i64,
            line,
            col,
        )
    }
}

/// Pop the top TS call frame from the trace stack. Wraps trace.pop_frame.
#[rtse::function("__RTS_FN_NS_ENGINE_TRACE_POP")]
fn trace_pop() {
    unsafe { __RTS_FN_NS_TRACE_POP_FRAME() }
}

/// Capture + render the current trace stack to a string handle. Wraps
/// trace.capture.
#[rtse::function("__RTS_FN_NS_ENGINE_TRACE_CAPTURE")]
fn trace_capture() -> Handle {
    unsafe { __RTS_FN_NS_TRACE_CAPTURE() }
}

/// Print the current trace stack to stderr. Wraps trace.print.
#[rtse::function("__RTS_FN_NS_ENGINE_TRACE_PRINT")]
fn trace_print() {
    unsafe { __RTS_FN_NS_TRACE_PRINT() }
}

/// Registra a namespace PRIVADA `engine` no motor. Re-expõe arch/time/trace para o
/// prelude embutido do motor; marcada `.private()` para não vazar pro código do
/// usuário.
pub fn register(e: &mut Engine) {
    e.module("engine", |m| {
        m.doc("PRIVATE engine-internal surface: arch + timestamps + trace, for the embedded TS prelude only.");
        m.private();
        m.registry(arch_entry());
        m.member(func(
            "is_buffer",
            "__RTS_FN_NS_ENGINE_IS_BUFFER",
            Sig::new(vec![AbiType::PolyValue], AbiType::PolyValue),
            "is_buffer(x: unknown): boolean",
            "Whether the value wraps an Entry::Buffer (an ArrayBuffer) — the structuredClone transfer bridge.",
            rts_shared::buffer::__RTS_FN_NS_ENGINE_IS_BUFFER as *const u8,
        ));
        m.member(func(
            "buffer_clone",
            "__RTS_FN_NS_ENGINE_BUFFER_CLONE",
            Sig::new(vec![AbiType::PolyValue], AbiType::PolyValue),
            "buffer_clone(x: unknown): unknown",
            "A NEW ArrayBuffer copying the bytes (structuredClone of a buffer).",
            rts_shared::buffer::__RTS_FN_NS_ENGINE_BUFFER_CLONE as *const u8,
        ));
        m.member(func(
            "buffer_detach",
            "__RTS_FN_NS_ENGINE_BUFFER_DETACH",
            Sig::new(vec![AbiType::PolyValue], AbiType::PolyValue),
            "buffer_detach(x: unknown): void",
            "Empty the buffer in place (JS detach: byteLength reads 0 afterwards).",
            rts_shared::buffer::__RTS_FN_NS_ENGINE_BUFFER_DETACH as *const u8,
        ));
        m.registry(now_ms_entry());
        m.registry(now_ns_entry());
        m.registry(unix_ms_entry());
        m.registry(unix_ns_entry());
        m.registry(trace_push_entry());
        m.registry(trace_pop_entry());
        m.registry(trace_capture_entry());
        m.registry(trace_print_entry());
    });
}
