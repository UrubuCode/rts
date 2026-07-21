//! `trace` namespace — RTS stack trace and debug tooling.
//!
//! `frame_stack` stays a public submodule: it is also consumed by
//! `gc::error` / `globals::error` to attach a Bun-style stack to thrown errors.
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).

pub mod frame_stack;

use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, sig};

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle};

/// Push a TS call frame onto the trace stack.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_PUSH_FRAME(
    file_ptr: *const u8,
    file_len: i64,
    fn_name_ptr: *const u8,
    fn_name_len: i64,
    line: i64,
    col: i64,
) {
    let file = match unsafe { rts_engine::abi::str_abi::from_abi(file_ptr, file_len) } {
        Some(s) => s,
        None => return,
    };
    let fn_name = match unsafe { rts_engine::abi::str_abi::from_abi(fn_name_ptr, fn_name_len) } {
        Some(s) => s,
        None => return,
    };
    frame_stack::push(
        file.to_string(),
        fn_name.to_string(),
        line as u32,
        col as u32,
    );
}

/// Pop the top TS call frame from the trace stack.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_POP_FRAME() {
    frame_stack::pop();
}

/// Capture current trace as a GC string handle. Returns 0 if stack is empty.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_CAPTURE() -> u64 {
    let s = frame_stack::capture_string();
    if s.is_empty() {
        return 0;
    }
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Print current trace stack to stderr.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_PRINT() {
    let s = frame_stack::format_stack();
    if s.is_empty() {
        eprintln!("<no trace frames>");
    } else {
        eprint!("{s}");
    }
}

/// Returns current trace stack depth.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_DEPTH() -> i64 {
    frame_stack::depth() as i64
}

/// Free a captured trace handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_FREE(handle: u64) {
    let _ = free_handle(handle);
}

fn func(
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
        emit: None,
    }
}

/// Registra a namespace `trace` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.ns("trace")
        .doc("RTS stack trace and debug tooling. Push/pop TS call frames; capture trace without error.")
        .member(func(
            "push_frame",
            "__RTS_FN_NS_TRACE_PUSH_FRAME",
            sig!(StrPtr, StrPtr, I64, I64 => Void),
            "push_frame(file: string, fn_name: string, line: number, col: number): void",
            "Push a TS call frame onto the trace stack.",
            __RTS_FN_NS_TRACE_PUSH_FRAME as *const u8,
        ))
        .member(func(
            "pop_frame",
            "__RTS_FN_NS_TRACE_POP_FRAME",
            sig!(=> Void),
            "pop_frame(): void",
            "Pop the top TS call frame from the trace stack.",
            __RTS_FN_NS_TRACE_POP_FRAME as *const u8,
        ))
        .member(func(
            "capture",
            "__RTS_FN_NS_TRACE_CAPTURE",
            sig!(=> Handle),
            "capture(): string",
            "Capture current trace as a GC string handle. Returns 0 if stack is empty.",
            __RTS_FN_NS_TRACE_CAPTURE as *const u8,
        ))
        .member(func(
            "print",
            "__RTS_FN_NS_TRACE_PRINT",
            sig!(=> Void),
            "print(): void",
            "Print current trace stack to stderr.",
            __RTS_FN_NS_TRACE_PRINT as *const u8,
        ))
        .member(func(
            "depth",
            "__RTS_FN_NS_TRACE_DEPTH",
            sig!(=> I64),
            "depth(): number",
            "Returns current trace stack depth.",
            __RTS_FN_NS_TRACE_DEPTH as *const u8,
        ))
        .member(func(
            "free",
            "__RTS_FN_NS_TRACE_FREE",
            sig!(Handle => Void),
            "free(handle: number): void",
            "Free a captured trace handle.",
            __RTS_FN_NS_TRACE_FREE as *const u8,
        ))
        .done();
}
