//! GC collection — CONSERVATIVE mark+sweep over the native stack.
//!
//! ## How it works
//!
//! 1. Every `GC_TICK_INTERVAL` allocations, `handles::alloc_entry` fires
//!    [`finish_cycle`] via the `GC_COLLECT_HOOK` (installed by [`runtime_init`]
//!    at startup — the engine can't name `rts-std`'s `finish_cycle` directly).
//! 2. [`finish_cycle`] calls [`super::scan::scan_all_roots`], the CONSERVATIVE
//!    scanner: it walks `rsp..stack_high` word-by-word (+ other RTS threads via
//!    SuspendThread + global cells) and treats any word with a non-zero handle
//!    generation as a root candidate, marking it (+ transitive children).
//! 3. `sweep_all_shards()` frees every handle that was NOT marked.
//!
//! The scanner needs NO Cranelift stack maps — it reads raw stack words — so the
//! exact same cycle runs for JIT frames AND for an AOT-compiled native binary
//! (`rts compile`). It works on x86-64; on other arches the scanner is a no-op,
//! so `finish_cycle` skips the whole cycle there (a sweep without a mark would
//! collect live stack handles), leaving explicit-free as the only reclamation.
//!
//! False positives (a non-handle stack word whose bits happen to have a non-zero
//! generation) only KEEP a slot alive one extra cycle — never corruption. Real
//! pointers are never confused with handles: the 48-bit payload is a HandleTable
//! slot index, not an address.

use super::handles::{live_handle_count, mark_handle, sweep_all_shards};

// ─── Module-level mutable global cells ────────────────────────────────────────
//
// A top-level `let` that is WRITTEN from inside a function is promoted by the new
// engine's front-end to a CELL: every access (top-level + the capturing function)
// goes through GCELL_GET/SET by a compile-time id, sidestepping the by-value
// capture limitation (epic #195). Each slot holds a PolyValue word; the front-end
// always SETs (the `let` initializer) before any GET, so an out-of-range GET
// (returns 0) never happens for a well-formed program. `mark_gcell_roots` keeps a
// boxed-handle slot's HandleTable entry alive across a GC cycle.

// THREAD-LOCAL (like the microtask queue): each program runs on its own thread,
// and the cell ids restart at 0 per compiled program — a process-global store
// would let concurrent in-process programs (the parallel unit tests) clobber each
// other's ids. The GC `finish_cycle` runs on the allocating (program) thread, so
// `mark_gcell_roots` sees the right thread's cells.
thread_local! {
    static GCELLS: std::cell::RefCell<Vec<u64>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Store `word` (a PolyValue) into global cell `id`, growing the store as needed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GCELL_SET(id: u64, word: u64) {
    GCELLS.with(|c| {
        let mut v = c.borrow_mut();
        let i = id as usize;
        if i >= v.len() {
            v.resize(i + 1, 0);
        }
        v[i] = word;
    });
}

/// Load global cell `id` (0 if never set — the front-end SETs before GET).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_GCELL_GET(id: u64) -> u64 {
    GCELLS.with(|c| c.borrow().get(id as usize).copied().unwrap_or(0))
}

/// Snapshot the CURRENT thread's cells (the spawning thread's module-globals) so a
/// spawned worker can inherit them — `GCELLS` is thread_local (program isolation),
/// so a worker thread otherwise sees an EMPTY store and reads every module-global
/// as `0`. A module-level `let`/`const` is SHARED across a program's threads in JS,
/// so `thread.spawn`/`parallel` snapshot here and [`gcell_restore`] in the worker.
/// The values are PolyValue words; a boxed handle stays valid cross-thread (the
/// HandleTable is process-global). Worker WRITES do not propagate back (workers
/// share via atomics/handles, not gcell writes — the read-only-shared case).
pub fn gcell_snapshot() -> Vec<u64> {
    GCELLS.with(|c| c.borrow().clone())
}

/// Install `snap` as the current (worker) thread's cells — see [`gcell_snapshot`].
pub fn gcell_restore(snap: Vec<u64>) {
    GCELLS.with(|c| *c.borrow_mut() = snap);
}

/// Mark every cell's PolyValue word as a GC root. A boxed STR/OBJECT/FUNCTION word
/// keeps its slot alive; an inline int/float/singleton word is a no-op inside
/// `mark_handle`. Called from `finish_cycle` alongside the microtask roots.
pub fn mark_gcell_roots() {
    GCELLS.with(|c| {
        for &w in c.borrow().iter() {
            mark_handle(w);
        }
    });
}

// ─── Core collector ──────────────────────────────────────────────────────────

/// Conservatively scan the native stack (+ other RTS threads + global cells),
/// mark every reachable GC handle, then sweep all unmarked handles.
///
/// The scanner ([`super::scan::scan_all_roots`]) is CONSERVATIVE: it walks
/// `rsp..stack_high` word-by-word and treats any word with a non-zero handle
/// generation as a root candidate. It does NOT consult the JIT
/// `stack_map_registry` — so it works identically for JIT-compiled frames AND
/// for AOT-compiled native binaries (`rts compile`), where the registry is
/// always empty.
///
/// Runs the full cycle on x86-64 (where the conservative scanner is implemented).
/// On non-x86-64 targets the scanner is a no-op, and a sweep WITHOUT a preceding
/// mark would collect live stack handles (heap corruption / truncated output —
/// seen on CI macOS arm64, test `js_parity_epic226`), so the whole cycle is
/// skipped there: handles stay live until explicit-free (more memory, but
/// correctness preserved).
pub fn finish_cycle() {
    // Não-x86_64: scanner é no-op → pular o ciclo (sweep sem mark = coletar vivos).
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

    // Compile-time string constants: the JIT/AOT lowering interns each string
    // LITERAL once and uses its handle as a code immediate (not on any stack /
    // cell / global), so the conservative scanner can't see it. Pinned roots
    // keep them alive for the whole program (their correct lifetime).
    super::handles::mark_pinned_roots();

    sweep_all_shards();
}

/// Incremental GC step. Currently a no-op — incremental pacing is a follow-up.
pub fn collect_debt() {}

// ─── Runtime bootstrap (JIT host-side + AOT main shim) ───────────────────────

/// One-time runtime bootstrap, run once at program startup BEFORE `__rtsn_main`.
///
/// Wires the two things the automatic GC needs that nothing else does anymore:
///
/// 1. **Installs the GC tick hook.** `handles::alloc_entry` fires
///    [`finish_cycle`] every `GC_TICK_INTERVAL` allocations through the
///    `GC_COLLECT_HOOK` indirection (the engine can't name `rts-std`'s
///    `finish_cycle` directly). The OLD engine's `jit.rs` installed this; the P5
///    cutover deleted that file and the install was never reconnected, so the
///    periodic GC silently never ran — an allocation-heavy loop (e.g. a UI/game
///    frame loop) leaked handles until the 5M abort. This reinstalls it.
/// 2. **Registers the main thread** in the `thread_registry`, so the conservative
///    root scanner ([`super::scan::scan_all_roots`]) covers the main thread's
///    stack via the same path it already uses for worker threads. Workers
///    self-register on spawn; the main thread had no one to register it (a
///    no-op for the common single-thread program, but correct for GC that
///    suspends+scans other threads).
///
/// Idempotent: the hook install is a `OnceLock::set` (later calls are ignored)
/// and `register_current` de-dups by thread id. Safe to call from both the JIT
/// host path and the AOT `main` shim.
pub fn runtime_init() {
    super::handles::install_gc_hook(finish_cycle);
    super::thread_registry::register_current();
}

// NB: the `extern "C" __RTS_FN_RT_INIT` symbol the AOT `main` shim calls lives in
// the `rts-runtime` FACADE — it also installs the UI render/input backend, which
// is in `rts-egui` (a crate `rts-std` does not depend on). The facade symbol
// calls [`runtime_init`] here plus the egui backend install, so the engine names
// ONE generic `RT`-scope bootstrap symbol and nothing egui-specific.

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
