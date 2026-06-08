//! `trace` namespace — RTS stack trace and debug tooling.
//!
//! `frame_stack` stays a public submodule: it is also consumed by
//! `gc::error` / `globals::error` to attach a Bun-style stack to thrown errors.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

pub mod frame_stack;

use rts_abi::ty::{Handle, I64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{Entry, alloc_entry, free_handle};

/// RTS stack trace and debug tooling. Push/pop TS call frames; capture trace without error.
#[rts_namespace(trace)]
impl TraceNs {
    /// Push a TS call frame onto the trace stack.
    #[rts_fn]
    pub fn push_frame(file: Str, fn_name: Str, line: I64, col: I64) {
        frame_stack::push(
            file.to_string(),
            fn_name.to_string(),
            line as u32,
            col as u32,
        );
    }

    /// Pop the top TS call frame from the trace stack.
    #[rts_fn]
    pub fn pop_frame() {
        frame_stack::pop();
    }

    /// Capture current trace as a GC string handle. Returns 0 if stack is empty.
    #[rts_fn]
    pub fn capture() -> Handle {
        let s = frame_stack::capture_string();
        if s.is_empty() {
            return 0;
        }
        alloc_entry(Entry::String(s.into_bytes()))
    }

    /// Print current trace stack to stderr.
    #[rts_fn]
    pub fn print() {
        let s = frame_stack::format_stack();
        if s.is_empty() {
            eprintln!("<no trace frames>");
        } else {
            eprint!("{s}");
        }
    }

    /// Returns current trace stack depth.
    #[rts_fn]
    pub fn depth() -> I64 {
        frame_stack::depth() as i64
    }

    /// Free a captured trace handle.
    #[rts_fn]
    pub fn free(handle: Handle) {
        let _ = free_handle(handle);
    }
}
