//! JIT GC stack map registry — precise liveness for JIT frames.
//!
//! Cranelift emits `UserStackMap` entries at every non-tail-call safepoint
//! when `declare_value_needs_stack_map` is used. After JIT
//! `finalize_definitions()`, the base addresses of each compiled function are
//! known and the per-function offsets can be resolved to absolute return PCs.
//!
//! The registry maps `return_pc → Vec<sp_offset>`. At GC time, the stack
//! walker reads the return address of each JIT frame, looks it up here, and
//! reads the GC handles at `SP + offset` to build the root set.
//!
//! # SP offset semantics
//!
//! `UserStackMap::entries()` yields `(ir::Type, sp_offset_from_sp)`. At the
//! return PC, SP is the caller's stack pointer at the call site. With
//! `preserve_frame_pointers=true` on x86-64:
//!
//! ```text
//!   callee RBP = caller SP - 16
//!   caller SP  = callee RBP + 16
//! ```
//!
//! The GC walker computes `caller_sp = frame_rbp + 16` and then reads
//! handles at `caller_sp + sp_offset`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Stack map entry collected from a single safepoint before the function
/// base address is known.
pub struct PendingEntry {
    pub func_id_raw: u32,
    /// `(ret_pc_offset_from_fn_start, sp_offsets)`
    pub maps: Vec<(u32, Vec<u32>)>,
}

static PENDING: OnceLock<Mutex<Vec<PendingEntry>>> = OnceLock::new();
static REGISTRY: OnceLock<Mutex<HashMap<usize, Vec<u32>>>> = OnceLock::new();

fn pending() -> &'static Mutex<Vec<PendingEntry>> {
    PENDING.get_or_init(|| Mutex::new(Vec::new()))
}

fn registry() -> &'static Mutex<HashMap<usize, Vec<u32>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Push pending stack map entries collected right after `define_function`.
/// `func_id_raw` is `FuncId::as_u32()`. Each entry in `maps` is a pair of
/// (return PC offset from function start, list of SP-relative offsets).
pub fn push_pending(func_id_raw: u32, maps: Vec<(u32, Vec<u32>)>) {
    if maps.is_empty() {
        return;
    }
    pending().lock().unwrap().push(PendingEntry { func_id_raw, maps });
}

/// Drain all pending entries. Called by `jit.rs` after `finalize_definitions()`
/// so callers can resolve function base addresses.
pub fn drain_pending() -> Vec<PendingEntry> {
    std::mem::take(&mut *pending().lock().unwrap())
}

/// Register a resolved mapping: `return_pc → sp_offsets`.
/// `return_pc` is the absolute address of the instruction after the `call`
/// (i.e., the return address the callee will see on its stack).
pub fn register(return_pc: usize, sp_offsets: Vec<u32>) {
    if sp_offsets.is_empty() {
        return;
    }
    registry().lock().unwrap().insert(return_pc, sp_offsets);
}

/// Look up SP-relative offsets for a given return PC.
/// Returns `None` if this PC is not a GC safepoint (non-JIT frame or call
/// with no live GC values).
pub fn lookup(return_pc: usize) -> Option<Vec<u32>> {
    registry().lock().unwrap().get(&return_pc).cloned()
}

/// True if the registry has at least one safepoint registered.
pub fn is_active() -> bool {
    registry()
        .lock()
        .unwrap()
        .len() > 0
}
