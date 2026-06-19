//! GC collection — precise mark+sweep for JIT frames via Cranelift stack maps.
//!
//! ## How it works
//!
//! 1. Codegen calls `builder.declare_value_needs_stack_map(val)` for every
//!    GC handle Value produced in a function.
//! 2. After each `define_function`, `jit.rs` extracts `UserStackMap` entries
//!    (via `ctx.compiled_code().buffer.user_stack_maps()`) and stores them in
//!    `stack_map_registry` keyed by per-function offset.
//! 3. After `finalize_definitions()`, absolute return-PC addresses are resolved
//!    and the registry is finalised.
//! 4. `finish_cycle()` walks the native stack (frame-pointer chain, valid because
//!    `preserve_frame_pointers=true`), looks up each return address in the
//!    registry, and marks every handle found at `caller_sp + offset` as a root.
//! 5. `sweep_all_shards()` frees every handle that was NOT marked.
//!
//! ## Fallback
//!
//! If the stack map registry has no entries (AOT path, or JIT before any maps
//! are registered), `finish_cycle()` is a no-op — the existing explicit-free
//! path remains the only reclamation mechanism. This preserves backwards
//! compatibility while the JIT path matures.

use super::handles::{live_handle_count, mark_handle, sweep_all_shards};
use super::stack_map_registry;

// ─── Module-level mutable global cells ────────────────────────────────────────
//
// A top-level `let` that is WRITTEN from inside a function is promoted by the new
// engine's front-end to a CELL: every access (top-level + the capturing function)
// goes through GCELL_GET/SET by a compile-time id, sidestepping the by-value
// capture limitation (epic #195). Each slot holds a PolyValue word; the front-end
// always SETs (the `let` initializer) before any GET, so an out-of-range GET
// (returns 0) never happens for a well-formed program. `mark_gcell_roots` keeps a
// boxed-handle slot's HandleTable entry alive across a GC cycle.

static GCELLS: std::sync::OnceLock<std::sync::Mutex<Vec<u64>>> = std::sync::OnceLock::new();

fn gcells() -> &'static std::sync::Mutex<Vec<u64>> {
    GCELLS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// Store `word` (a PolyValue) into global cell `id`, growing the store as needed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GCELL_SET(id: u64, word: u64) {
    let mut v = gcells().lock().unwrap();
    let i = id as usize;
    if i >= v.len() {
        v.resize(i + 1, 0);
    }
    v[i] = word;
}

/// Load global cell `id` (0 if never set — the front-end SETs before GET).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GCELL_GET(id: u64) -> u64 {
    gcells().lock().unwrap().get(id as usize).copied().unwrap_or(0)
}

/// Mark every cell's PolyValue word as a GC root. A boxed STR/OBJECT/FUNCTION word
/// keeps its slot alive; an inline int/float/singleton word is a no-op inside
/// `mark_handle`. Called from `finish_cycle` alongside the microtask roots.
pub fn mark_gcell_roots() {
    if let Some(m) = GCELLS.get() {
        for &w in m.lock().unwrap().iter() {
            mark_handle(w);
        }
    }
}

// ─── Core collector ──────────────────────────────────────────────────────────

/// Walk the native stack frame-by-frame, mark every GC handle that is live
/// at a JIT safepoint, then sweep all unmarked handles.
///
/// Only active when the stack map registry has been populated (i.e., at least
/// one JIT function with GC-tracked values has been compiled and finalised).
/// On non-x86-64 targets or when the registry is empty this is a no-op.
pub fn finish_cycle() {
    if !stack_map_registry::is_active() {
        return;
    }

    // Em arquiteturas onde o stack scanner nao roda (nao-x86_64),
    // skip o ciclo completo: sweep sem mark coleta handles vivos do
    // stack -> heap corruption / output truncado em testes (visto em
    // CI macOS arm64 com test js_parity_epic226).
    // Tradeoff: handles ficam vivos ate explicit-free; uso de memoria
    // aumenta mas correctness preservada.
    #[cfg(not(target_arch = "x86_64"))]
    {
        return;
    }

    // Acha as roots conservativas (stack da thread atual + outras threads +
    // globals) via o scanner do `rts-engine`, passando `mark_handle` como visitor
    // — `mark_handle` marca + propaga transitivamente. O scanner (asm/FFI) é puro
    // mecanismo no engine; a HandleTable tipada e esta orquestração ficam aqui.
    //
    // Safety: lê memória de stack crua. Precondições: `preserve_frame_pointers=
    // true` garante cadeia RBP válida nos frames JIT; só desreferenciamos dentro
    // dos limites de stack; candidatos inválidos são filtrados por geração + slot.
    unsafe { super::scan::scan_all_roots(&mut |c| mark_handle(c)) };

    // (cross-runtime #344/#393) Mark handles held only by pending microtasks
    // (async/Promise callback closures, generator drives) — they live in the
    // heap microtask queue, not on any scanned stack, so without this a GC tick
    // during synchronous code sweeps live async state.
    crate::globals::text_encoding::instance::mark_microtask_roots();

    // Module-level mutable globals (epic #195): keep boxed-handle cell contents
    // (e.g. an accumulating string) alive across the sweep.
    mark_gcell_roots();

    sweep_all_shards();
}

/// Incremental GC step. Currently a no-op — incremental pacing is a follow-up.
pub fn collect_debt() {}


// ─── GC entry points ─────────────────────────────────────────────────────────

/// Triggers a full mark+sweep cycle.
/// Returns the number of handles freed.
pub fn collect(_roots: &[u64]) -> u64 {
    let before = live_handle_count() as u64;
    finish_cycle();
    let after = live_handle_count() as u64;
    before.saturating_sub(after)
}

// ─── Extern ABI ──────────────────────────────────────────────────────────────

/// Full collection cycle triggered from userland (`gc.collect()`).
/// Returns handles swept.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT(root: u64) -> i64 {
    let _ = root;
    collect(&[]) as i64
}

/// Collects with a Vec of roots (legacy multi-root API — parameters ignored).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT_VEC(roots_vec: u64) -> i64 {
    let _ = roots_vec;
    collect(&[]) as i64
}

/// Incremental collection step. No-op until incremental pacing is implemented.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_COLLECT_DEBT() {
    collect_debt();
}

/// Live handle count. Useful for benchmarks and leak detection.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_LIVE_COUNT() -> i64 {
    live_handle_count() as i64
}
