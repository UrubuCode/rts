use cranelift_codegen::ir::StackSlot;

use crate::namespaces::abi::{
    FN_CRYPTO_SHA256, FN_GLOBAL_DELETE, FN_GLOBAL_GET, FN_GLOBAL_HAS, FN_GLOBAL_SET, FN_IO_PANIC,
    FN_IO_PRINT, FN_IO_STDERR_WRITE, FN_IO_STDOUT_WRITE, FN_PROCESS_EXIT,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VRegKind {
    Handle,
    NativeF64,
    NativeI32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BindingState {
    pub(super) slot: StackSlot,
    pub(super) mutable: bool,
    pub(super) kind: VRegKind,
}

pub(super) const ABI_ARG_SLOTS: usize = 6;
pub(super) const ABI_PARAM_COUNT: usize = ABI_ARG_SLOTS + 1;
pub(super) const ABI_UNDEFINED_HANDLE: i64 = 0;

pub(super) const RTS_DISPATCH: &str = "__rts_dispatch";

pub(super) const CALLEE_FN_IDS: &[(&str, i64)] = &[
    ("io.print", FN_IO_PRINT),
    ("io.stdout_write", FN_IO_STDOUT_WRITE),
    ("io.stderr_write", FN_IO_STDERR_WRITE),
    ("io.panic", FN_IO_PANIC),
    ("crypto.sha256", FN_CRYPTO_SHA256),
    ("process.exit", FN_PROCESS_EXIT),
    ("globals.set", FN_GLOBAL_SET),
    ("globals.get", FN_GLOBAL_GET),
    ("globals.has", FN_GLOBAL_HAS),
    ("globals.remove", FN_GLOBAL_DELETE),
];
