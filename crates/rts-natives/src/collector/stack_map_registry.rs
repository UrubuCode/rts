//! JIT GC stack map registry — the transport for precise liveness of JIT frames.
//!
//! Cranelift emits `UserStackMap` entries at every non-tail-call safepoint for
//! values declared with `declare_value_needs_stack_map`. After JIT
//! `finalize_definitions()` the base address of each compiled function is known,
//! so per-function offsets can be resolved to absolute return PCs.
//!
//! The registry maps `return_pc → Vec<sp_offset>`. At GC time a precise walker
//! would read the return address of each JIT frame, look it up here, and read
//! the GC handles at `SP + offset` to build the root set.
//!
//! # HONEST STATE: this transport currently carries NOTHING
//!
//! Nothing calls `declare_value_needs_stack_map` — the occurrences in the tree
//! are all comments — and [`lookup`] has no callers. So the producers push empty
//! sets, `module_jit` drains empty sets, and the scanner
//! ([`super::scan::scan_all_roots`]) is fully CONSERVATIVE. `CLAUDE.md` and
//! `.claude/rules/02-runtime.md` claimed otherwise until 2026-07-31; they now
//! say this. Wiring it for real is `RTS_ORGANIZATION.md` N6.
//!
//! # Why collection is SESSION-SCOPED (N6 phase A)
//!
//! `PENDING` used to be one process-global `Vec` that anything could push to and
//! `module_jit::compile_program` unconditionally drained. That is a latent
//! crash, and it becomes a live one the moment a single value is declared:
//!
//! * `bake.rs::capture_compiled` builds a SEPARATE, throwaway `JITModule` and
//!   never drains. `FuncId` numbering restarts per module, so its `FuncId(42)`
//!   and the real run's `FuncId(42)` are unrelated functions.
//! * `module_aot.rs` pushes through the same `parcompile` path and never drains
//!   either.
//! * The next real `compile_program` would then drain those foreign entries and
//!   call `get_finalized_function` on a `FuncId` belonging to another module —
//!   which either **panics** or silently resolves to a different function,
//!   producing garbage return PCs. Under precise scanning a garbage PC means a
//!   frame reports the wrong roots or none at all, and reporting no roots is a
//!   PREMATURE FREE. Conservative scanning over-approximates and is safe;
//!   precise scanning under-approximates and is not.
//!
//! So a push is only recorded while a [`CollectionGuard`] is alive on this
//! thread, and only the module that opened the guard can drain it. A producer
//! that will not drain — the bake scratch module, AOT — simply never opens one,
//! and its pushes are discarded at the source instead of poisoning a global.
//!
//! The guard SAVES AND RESTORES the previous buffer, so nesting is safe:
//! `dynfn::compile_dynamic_fn` (`new Function`) compiles a fresh module while a
//! program is already running.
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
//! The GC walker computes `caller_sp = frame_rbp + 16` and then reads handles at
//! `caller_sp + sp_offset`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Stack map entry collected from a single safepoint before the function base
/// address is known.
pub struct PendingEntry {
    pub func_id_raw: u32,
    /// `(ret_pc_offset_from_fn_start, sp_offsets)`
    pub maps: Vec<(u32, Vec<u32>)>,
}

thread_local! {
    /// The open collection, if any. `None` means nothing on this thread is
    /// collecting and [`push_pending`] discards — which is the correct answer
    /// for a module that will never resolve the entries.
    ///
    /// Thread-local rather than global because it is per-COMPILATION, and the
    /// push happens in `parcompile`'s SERIAL define loop on the compiling
    /// thread (the parallel phase only produces bytes; it does not push).
    static PENDING: RefCell<Option<Vec<PendingEntry>>> = const { RefCell::new(None) };
}

static REGISTRY: OnceLock<Mutex<HashMap<usize, Vec<u32>>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<usize, Vec<u32>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Open a collection for the current compilation. Entries pushed while this is
/// alive are visible to [`drain_pending`]; entries pushed with no guard alive
/// are discarded.
///
/// Held by the module that will call `finalize_definitions()` and resolve the
/// entries — today only `module_jit::compile_program`.
#[must_use = "the collection closes when the guard is dropped"]
pub struct CollectionGuard {
    /// Whatever collection was open before, restored on drop so a nested
    /// compilation (`new Function` while a program runs) cannot eat the outer
    /// one's entries.
    previous: Option<Vec<PendingEntry>>,
}

/// Begin collecting stack-map entries on this thread. See [`CollectionGuard`].
pub fn begin_collection() -> CollectionGuard {
    let previous = PENDING.with(|p| p.borrow_mut().replace(Vec::new()));
    CollectionGuard { previous }
}

impl Drop for CollectionGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        PENDING.with(|p| *p.borrow_mut() = previous);
    }
}

/// Record a function's stack-map entries, keyed by `FuncId::as_u32()`, for the
/// open collection. Each entry in `maps` is `(return PC offset from function
/// start, SP-relative offsets)`.
///
/// **Discards when no [`CollectionGuard`] is alive on this thread.** That is not
/// a failure mode — it is how a module that will never resolve these entries
/// (the bake scratch module, AOT) avoids handing another module's `FuncId`
/// numbering to a resolver. See the module doc.
pub fn push_pending(func_id_raw: u32, maps: Vec<(u32, Vec<u32>)>) {
    if maps.is_empty() {
        return;
    }
    PENDING.with(|p| {
        if let Some(buf) = p.borrow_mut().as_mut() {
            buf.push(PendingEntry { func_id_raw, maps });
        }
    });
}

/// Take everything collected so far in the open collection. Called after
/// `finalize_definitions()`, when function base addresses are known.
///
/// Returns empty when no collection is open, so a caller that forgot the guard
/// resolves nothing rather than resolving somebody else's `FuncId`s.
pub fn drain_pending() -> Vec<PendingEntry> {
    PENDING.with(|p| {
        p.borrow_mut()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    })
}

/// Register a resolved mapping: `return_pc → sp_offsets`. `return_pc` is the
/// absolute address of the instruction after the `call` — the return address
/// the callee will see on its stack.
pub fn register(return_pc: usize, sp_offsets: Vec<u32>) {
    if sp_offsets.is_empty() {
        return;
    }
    registry().lock().unwrap().insert(return_pc, sp_offsets);
}

/// Look up SP-relative offsets for a return PC. `None` when the PC is not a GC
/// safepoint (a non-JIT frame, or a call with no live GC values).
///
/// UNUSED today — the scanner is conservative and never consults this. Before a
/// precise walker calls it, note the lock: `scan.rs` documents that only
/// lock-free work is allowed while another thread is SUSPENDED, so this must be
/// called after `ResumeThread` or `REGISTRY` must become a read-mostly snapshot.
pub fn lookup(return_pc: usize) -> Option<Vec<u32>> {
    registry().lock().unwrap().get(&return_pc).cloned()
}

/// True if at least one safepoint is registered.
pub fn is_active() -> bool {
    !registry().lock().unwrap().is_empty()
}
