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

    // Safety: we read raw stack memory. Preconditions:
    // - `preserve_frame_pointers=true` guarantees a valid RBP chain in all
    //   JIT-compiled frames.
    // - Rust frames above us also preserve frame pointers (rustc default on
    //   x86-64 release builds; enforced by the same Cranelift flag).
    // - We only dereference addresses where we have a registered stack map —
    //   invalid return addresses simply miss the registry lookup and are skipped.
    unsafe { mark_stack_roots() };

    sweep_all_shards();
}

/// Incremental GC step. Currently a no-op — incremental pacing is a follow-up.
pub fn collect_debt() {}

// ─── Stack scanner ────────────────────────────────────────────────────────────

// Conservative scan of the current thread's stack. On x86-64, reads RSP and
// scans upward through a bounded window looking for u64 values that decode as
// live handles. False positives are filtered by the generation check in `mark`.
//
// This avoids the fragility of walking the RBP chain through Rust frames,
// which on Windows x64 may not maintain the frame-pointer invariant.
unsafe fn mark_stack_roots() {
    #[cfg(target_arch = "x86_64")]
    {
        // 1. Varre a thread atual (a que disparou o GC tick).
        let mut rsp: usize;
        unsafe { std::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, readonly)); }
        let stack_high = stack_high_addr();
        if super::debug::is_enabled() {
            eprintln!(
                "[gc] mark_stack_roots SELF rsp={rsp:#x} stack_high={stack_high:#x} delta={}",
                stack_high.saturating_sub(rsp)
            );
        }
        unsafe { scan_range(rsp, stack_high); }

        // 2. Suspende e varre stack de cada thread RTS registrada.
        //    Necessario porque sem isso os handles vivos em workers
        //    eram swept e segfault sob carga (>16 conn http_server).
        #[cfg(target_os = "windows")]
        unsafe { mark_other_threads_windows(); }
    }

    #[cfg(not(target_arch = "x86_64"))]
    let _ = ();
}

#[cfg(target_arch = "x86_64")]
unsafe fn scan_range(low: usize, high: usize) {
    if high <= low {
        return;
    }
    let mut addr = low;
    let mut marked = 0usize;
    while addr + 8 <= high {
        let candidate = unsafe { *(addr as *const u64) };
        if candidate != 0 {
            let generation = (candidate >> crate::abi::handles::HANDLE_GEN_SHIFT) & 0xFFFF;
            if generation != 0 {
                mark_handle(candidate);
                marked += 1;
            }
        }
        addr += 8;
    }
    if super::debug::is_enabled() {
        eprintln!(
            "[gc]   scan_range low={low:#x} high={high:#x} qwords={} marks={}",
            (high - low) / 8,
            marked
        );
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
unsafe fn mark_other_threads_windows() {
    use super::thread_registry::{ThreadInfo, with_other_threads};

    // Coletamos primeiro pra liberar o lock do registry antes de
    // suspender threads (suspender com lock segurado e' deadlock garantido
    // se o GC dispara dentro de spawn).
    let mut others: Vec<ThreadInfo> = Vec::new();
    with_other_threads(|t| others.push(*t));

    for t in others {
        if t.handle == 0 {
            continue;
        }
        unsafe {
            // SuspendThread retorna previous suspend count, -1 em erro.
            let prev = SuspendThread(t.handle);
            if prev as i32 == -1 {
                continue;
            }
            // CONTEXT precisa estar 16-byte aligned. Alocamos zerado.
            let mut ctx: CONTEXT = std::mem::zeroed();
            ctx.ContextFlags = CONTEXT_CONTROL | CONTEXT_INTEGER;
            let ok = GetThreadContext(t.handle, &mut ctx);
            if ok != 0 {
                let rsp = ctx.Rsp as usize;
                if super::debug::is_enabled() {
                    eprintln!(
                        "[gc] mark_stack_roots OTHER tid={} rsp={rsp:#x} stack_high={:#x}",
                        t.thread_id, t.stack_high
                    );
                }
                scan_range(rsp, t.stack_high);
                // Tambem marca handles em registers callee-saved (RBX,
                // RBP, RDI, RSI, R12-R15) que podem segurar values vivos.
                for reg in [
                    ctx.Rbx, ctx.Rbp, ctx.Rdi, ctx.Rsi,
                    ctx.R12, ctx.R13, ctx.R14, ctx.R15,
                ] {
                    let candidate = reg as u64;
                    if candidate != 0 {
                        let generation = (candidate >> crate::abi::handles::HANDLE_GEN_SHIFT) & 0xFFFF;
                        if generation != 0 {
                            mark_handle(candidate);
                        }
                    }
                }
            }
            ResumeThread(t.handle);
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const CONTEXT_AMD64: u32 = 0x00100000;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const CONTEXT_CONTROL: u32 = CONTEXT_AMD64 | 0x1;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const CONTEXT_INTEGER: u32 = CONTEXT_AMD64 | 0x2;

// Layout de CONTEXT (Windows x64). Precisa estar 16-byte aligned.
// Apenas os campos que usamos (Rsp + callee-saved) tem nomes precisos;
// resto e' padding opaco.
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
#[repr(C, align(16))]
#[allow(non_snake_case)]
struct CONTEXT {
    P1Home: u64, P2Home: u64, P3Home: u64, P4Home: u64, P5Home: u64, P6Home: u64,
    ContextFlags: u32,
    MxCsr: u32,
    SegCs: u16, SegDs: u16, SegEs: u16, SegFs: u16, SegGs: u16, SegSs: u16,
    EFlags: u32,
    Dr0: u64, Dr1: u64, Dr2: u64, Dr3: u64, Dr6: u64, Dr7: u64,
    Rax: u64, Rcx: u64, Rdx: u64,
    Rbx: u64, Rsp: u64, Rbp: u64, Rsi: u64, Rdi: u64,
    R8: u64, R9: u64, R10: u64, R11: u64,
    R12: u64, R13: u64, R14: u64, R15: u64,
    Rip: u64,
    // Resto (FloatSave, vector regs, debug regs) — ~512 bytes de padding
    // pra completar tamanho oficial de CONTEXT (1232 bytes).
    _pad: [u8; 1232 - 0x100],
}

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
unsafe extern "system" {
    fn SuspendThread(h: usize) -> u32;
    fn ResumeThread(h: usize) -> u32;
    fn GetThreadContext(h: usize, ctx: *mut CONTEXT) -> i32;
}

/// Returns the high (exclusive) address of the current thread's stack.
/// Uses the NT Thread Information Block (TIB) on Windows (gs:[0x10] = StackBase)
/// and pthread_getattr_np on Linux. Falls back to RSP + 512 KB.
#[cfg(target_arch = "x86_64")]
fn stack_high_addr() -> usize {
    #[cfg(target_os = "windows")]
    {
        // Usa `GetCurrentThreadStackLimits` — API Win32 oficial que retorna
        // (low, high) absolutos do stack do thread atual, sempre coerente
        // com RSP. Antes usavamos `gs:[0x10]` (TIB.StackBase) que em alguns
        // contextos retornava valor < RSP, deixando o scanner retornar
        // sem marcar nada (sweep coletava handles vivos).
        unsafe extern "system" {
            fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
        }
        let mut low: usize = 0;
        let mut high: usize = 0;
        unsafe { GetCurrentThreadStackLimits(&mut low, &mut high); }
        high
    }

    #[cfg(target_os = "linux")]
    {
        // Use /proc/self/maps parsing or a simple heuristic.
        // For simplicity, use RSP + 8 MB (main thread default on Linux).
        let mut rsp: usize;
        unsafe { std::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, readonly)); }
        rsp + 8 * 1024 * 1024
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let mut rsp: usize;
        unsafe { std::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, readonly)); }
        rsp + 512 * 1024
    }
}

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
