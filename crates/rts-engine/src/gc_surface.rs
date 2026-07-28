//! Single canonical forward-declaration site for the small set of
//! `#[unsafe(no_mangle)] extern "C"` collector/runtime symbols that many
//! crates above `rts-engine` need to call, but whose real definition does
//! NOT live in `rts-engine` (most live in `rts-std`'s collector, some in
//! `rts-primitives`).
//!
//! These symbols do not migrate down into `rts-engine` — they carry
//! backend/state or class-level concerns (the GC string pool, the
//! thread-local pending-error slot, the generator/async state-machine
//! drain, Function/Promise class operations) that belong in the layer that
//! owns them. Every consumer instead resolves them **by link**: the JIT
//! registers the symbol in its symbol table and the AOT staticlib embeds
//! the `#[no_mangle]` export, so a plain `unsafe extern "C" { ... }`
//! forward declaration is enough for the linker (or the JIT loader) to
//! bind it at load/link time, whichever layer the real body lives in.
//!
//! Before this module existed, ~15 crates each hand-rolled their own copy
//! of these declarations (three near-identical `gc_surface.rs` files plus
//! one-off blocks scattered across `rts-node`/`rts-napi`/`rts-egui`/
//! `rts-input`/`rts-dom`/`rts-shared`/`rts-std`), and they drifted (`fn` vs
//! `pub safe fn`, partial subsets) — tripping `clashing_extern_declarations`.
//! `rts-engine` is the right home for the ONE declaration: it sits at the
//! bottom of the dependency graph (`rts-engine <- rts-primitives + rts-shared
//! <- rts-std <- rts-runtime`, with `rts-node`/`rts-napi`/`rts-egui`/
//! `rts-input`/`rts-dom` also depending on it directly), so every consumer
//! can reach it without violating layering, regardless of which layer above
//! actually defines the symbol's body.
//!
//! `pub safe fn` (edition 2024): the real definitions are plain Rust
//! `extern "C"` functions with no caller invariants to uphold, so declaring
//! them `safe` here preserves the original call-site ergonomics (no
//! `unsafe` block needed to call them).
//!
//! `__RTS_FN_NS_GC_PIN_HANDLE` / `__RTS_FN_NS_GC_UNPIN_HANDLE` are NOT
//! forward-declared here — their real definition lives in this very crate
//! (`crate::heap::handles`), so they are re-exported as ordinary items
//! instead of round-tripped through an extern-link declaration.

pub use crate::heap::handles::{__RTS_FN_NS_GC_PIN_HANDLE, __RTS_FN_NS_GC_UNPIN_HANDLE};

unsafe extern "C" {
    /// Allocate a GC string from UTF-8 bytes, return its handle. (`rts-std`
    /// `collector::string_pool`)
    pub safe fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;

    /// Concatenate two GC string handles at the raw byte level (no UTF-8
    /// re-decode — preserves incomplete multibyte fragments across the
    /// join). (`rts-std` `collector::string_pool`)
    pub safe fn __RTS_FN_NS_GC_STRING_CONCAT(a: u64, b: u64) -> u64;

    /// Coerce any handle to its JS string-handle representation. (`rts-std`
    /// `collector::string_pool`)
    pub safe fn __RTS_FN_RT_TO_STRING_HANDLE(handle: u64) -> u64;

    /// Set the pending error (thrown value) and capture the current call
    /// stack. (`rts-std` `collector::error`)
    pub safe fn __RTS_FN_RT_ERROR_SET(handle: u64);

    /// Drain a generator/async-generator's state machine to completion.
    /// (`rts-std` `collector::generator`)
    pub safe fn __RTS_FN_NS_GC_GEN_SM_DRAIN(h: u64) -> u64;

    /// Generic runtime-dispatch invocation of a Function-class handle:
    /// `callee(this_arg, ...args)`. (`rts-primitives` `function::ops`)
    pub safe fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;

    /// `Function.prototype.call`-style invocation of a Function-class
    /// handle. (`rts-primitives` `function::ops`)
    pub safe fn __RTS_FN_GL_FUNCTION_CALL(handle: u64, this_arg: i64, args_handle: u64) -> i64;

    /// Handle of the native Function backing `Array.prototype[Symbol.iterator]`.
    /// (`rts-std` `collector::generator`)
    pub safe fn __RTS_FN_GL_ARRAY_ITERATOR_FN() -> u64;

    /// Create an already-fulfilled Promise wrapping `value`, return its
    /// handle. (`rts-std` `globals::fetch::instance`)
    pub safe fn __RTS_FN_GL_PROMISE_RESOLVE(value: u64) -> i64;

    /// Create an already-rejected Promise wrapping `reason`, return its
    /// handle. (`rts-std` `globals::fetch::instance`)
    pub safe fn __RTS_FN_GL_PROMISE_REJECT(reason: u64) -> i64;
}
