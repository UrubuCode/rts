//! A minimal, self-contained runtime the JIT calls — the PolyValue ABI boundary.
//!
//! This is the P1 proof that `PolyValue` crosses the JIT↔runtime boundary
//! cleanly and that heterogeneous storage works with NO per-type boxing zoo (no
//! `Entry::FloatPrim`). It is deliberately tiny but real: a string interner
//! ([`strings`]), a `PolyValue` vector store ([`container`]), and the generic JS
//! operators ([`ops`]). Everything crossing `extern "C"` is a raw `u64` carrying
//! a [`crate::value::PolyValue`].
//!
//! [`symbols`] exposes the `(name, fn_ptr)` table the JIT harness
//! ([`crate::lower::jit`]) installs into the `JITBuilder` so the lowered code can
//! `call` these by symbol name — a hand-rolled stand-in for the full
//! SPECS-derived `abi_gen` table (P4).

pub mod console;
pub mod container;
pub mod ops;
pub mod strings;
pub mod tostring;

/// One installable runtime symbol: an extern "C" name and its function pointer.
#[derive(Clone, Copy)]
pub struct RuntimeSymbol {
    pub name: &'static str,
    pub ptr: *const u8,
}

// The pointers are to `#[no_mangle] extern "C"` functions with a static lifetime;
// sending the table across threads (the JIT installs it on whatever thread runs
// the test) is sound.
unsafe impl Send for RuntimeSymbol {}
unsafe impl Sync for RuntimeSymbol {}

/// The full table of `__rtsn_*` runtime symbols the JIT harness must install.
///
/// This is the explicit, hand-maintained P1 analogue of `abi_gen::jit_symbols()`
/// (which P4 will derive from `SPECS` with a build-time coverage assert). Every
/// `CallExtern` the lowering can emit must resolve to an entry here.
pub fn symbols() -> Vec<RuntimeSymbol> {
    vec![
        RuntimeSymbol { name: "__rtsn_add", ptr: ops::__rtsn_add as *const u8 },
        RuntimeSymbol { name: "__rtsn_strict_eq", ptr: ops::__rtsn_strict_eq as *const u8 },
        RuntimeSymbol { name: "__rtsn_strict_neq", ptr: ops::__rtsn_strict_neq as *const u8 },
        RuntimeSymbol { name: "__rtsn_typeof", ptr: ops::__rtsn_typeof as *const u8 },
        RuntimeSymbol { name: "__rtsn_to_string", ptr: ops::__rtsn_to_string as *const u8 },
        RuntimeSymbol { name: "__rtsn_to_boolean", ptr: ops::__rtsn_to_boolean as *const u8 },
        RuntimeSymbol { name: "__rtsn_vec_new", ptr: container::__rtsn_vec_new as *const u8 },
        RuntimeSymbol { name: "__rtsn_vec_push", ptr: container::__rtsn_vec_push as *const u8 },
        RuntimeSymbol { name: "__rtsn_vec_get", ptr: container::__rtsn_vec_get as *const u8 },
        RuntimeSymbol { name: "__rtsn_vec_len", ptr: container::__rtsn_vec_len as *const u8 },
        RuntimeSymbol { name: "__rtsn_console_log0", ptr: console::__rtsn_console_log0 as *const u8 },
        RuntimeSymbol { name: "__rtsn_console_log1", ptr: console::__rtsn_console_log1 as *const u8 },
        RuntimeSymbol { name: "__rtsn_console_log2", ptr: console::__rtsn_console_log2 as *const u8 },
        RuntimeSymbol { name: "__rtsn_console_log3", ptr: console::__rtsn_console_log3 as *const u8 },
        RuntimeSymbol { name: "__rtsn_console_log4", ptr: console::__rtsn_console_log4 as *const u8 },
        RuntimeSymbol { name: "__rtsn_console_log5", ptr: console::__rtsn_console_log5 as *const u8 },
        RuntimeSymbol { name: "__rtsn_console_log6", ptr: console::__rtsn_console_log6 as *const u8 },
    ]
}

/// Look up the arity (param count) of a known runtime symbol. The harness needs
/// this to build the imported-function signature (all params + ret are i64 in
/// the PolyValue ABI; `__rtsn_vec_push` returns nothing).
pub fn signature_of(name: &str) -> Option<ExternSig> {
    Some(match name {
        "__rtsn_add" | "__rtsn_strict_eq" | "__rtsn_strict_neq" => {
            ExternSig { params: 2, returns: true }
        }
        "__rtsn_typeof" | "__rtsn_to_string" | "__rtsn_to_boolean" => {
            ExternSig { params: 1, returns: true }
        }
        "__rtsn_vec_new" => ExternSig { params: 0, returns: true },
        "__rtsn_vec_push" => ExternSig { params: 2, returns: false },
        "__rtsn_vec_get" => ExternSig { params: 2, returns: true },
        "__rtsn_vec_len" => ExternSig { params: 1, returns: true },
        // console.log entries: N args, void return.
        "__rtsn_console_log0" => ExternSig { params: 0, returns: false },
        "__rtsn_console_log1" => ExternSig { params: 1, returns: false },
        "__rtsn_console_log2" => ExternSig { params: 2, returns: false },
        "__rtsn_console_log3" => ExternSig { params: 3, returns: false },
        "__rtsn_console_log4" => ExternSig { params: 4, returns: false },
        "__rtsn_console_log5" => ExternSig { params: 5, returns: false },
        "__rtsn_console_log6" => ExternSig { params: 6, returns: false },
        _ => return None,
    })
}

/// The i64-only ABI shape of a runtime extern: a param count and whether it
/// returns a value (all slots are i64 = a raw PolyValue word).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExternSig {
    pub params: usize,
    pub returns: bool,
}
