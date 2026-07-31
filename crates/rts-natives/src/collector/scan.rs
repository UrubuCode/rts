//! Scanner conservativo de roots do GC — o MECANISMO puro de root-finding
//! (asm RSP, varredura de ranges de stack, suspensão de outras threads, leitura
//! dos registries), parametrizado por um `visit` callback.
//!
//! **Não conhece a HandleTable.** O runtime chama [`scan_all_roots`] passando
//! `mark_handle` como `visit` — `mark_handle` marca o candidato E propaga
//! transitivamente (worklist próprio). Aqui só ACHAMOS os candidatos (qwords na
//! stack/registradores/globais cuja geração ≠ 0) e os entregamos ao `visit`.
//! A separação é o cerne do SPLIT: scanning genérico no `rts-engine`, heap
//! tipado (`Entry`) + orquestração de ciclo (`finish_cycle` com roots de domínio
//! tipo microtasks) no `rts-runtime`.

use super::{debug, global_roots, thread_registry};
use crate::abi::handles::HANDLE_GEN_SHIFT;

/// Varre todas as roots conservativas e chama `visit(candidate)` para cada
/// handle-candidato encontrado (stack da thread atual + outras threads RTS
/// registradas + globals top-level com handle).
///
/// `visit` é tipicamente `mark_handle` (marca + propaga). Em alvos não-x86_64 é
/// no-op (o `finish_cycle` do runtime já faz early-return nesses alvos para não
/// varrer sem mark e coletar vivos).
///
/// # Safety
/// Lê memória de stack crua. Precondições: `preserve_frame_pointers=true`
/// garante cadeia RBP válida nos frames JIT; só desreferenciamos endereços
/// dentro dos limites de stack conhecidos; candidatos inválidos são filtrados
/// pela checagem de geração + pela validação de slot no `mark`.
pub unsafe fn scan_all_roots(visit: &mut dyn FnMut(u64)) {
    #[cfg(target_arch = "x86_64")]
    {
        // 1. Varre a thread atual (a que disparou o GC tick).
        //
        // CRÍTICO: capturar os callee-saved (RBX/RBP/R12-R15) ANTES de varrer.
        // Quando o GC dispara via `alloc_entry` no meio de uma função JIT, um
        // handle vivo recém-criado pode estar SÓ num registrador callee-saved,
        // ainda não derramado na pilha — o `scan_range` (que só lê rsp..high) o
        // perderia e o sweep coletaria um valor vivo (string corrompida). Para
        // OUTRAS threads isso já é feito via GetThreadContext; para a thread
        // atual capturamos os registradores aqui e os tratamos como roots.
        // (RSI/RDI são caller-saved na convenção SysV/Win64 — não seguram value
        // vivo através de um `call` para `alloc_entry`, então bastam os 6
        // callee-saved; RBP entra porque com frame pointers pode segurar handle.)
        let mut rsp: usize;
        let mut s_rbx: u64;
        let mut s_rbp: u64;
        let mut s_r12: u64;
        let mut s_r13: u64;
        let mut s_r14: u64;
        let mut s_r15: u64;
        unsafe {
            std::arch::asm!(
                "mov {rsp}, rsp",
                "mov {rbx}, rbx",
                "mov {rbp}, rbp",
                "mov {r12}, r12",
                "mov {r13}, r13",
                "mov {r14}, r14",
                "mov {r15}, r15",
                rsp = out(reg) rsp,
                rbx = out(reg) s_rbx,
                rbp = out(reg) s_rbp,
                r12 = out(reg) s_r12,
                r13 = out(reg) s_r13,
                r14 = out(reg) s_r14,
                r15 = out(reg) s_r15,
                options(nostack, readonly),
            );
        }
        let saved: [u64; 6] = [s_rbx, s_rbp, s_r12, s_r13, s_r14, s_r15];
        let stack_high = stack_high_addr();
        if debug::is_enabled() {
            eprintln!(
                "[gc] scan_all_roots SELF rsp={rsp:#x} stack_high={stack_high:#x} delta={}",
                stack_high.saturating_sub(rsp)
            );
        }
        // Marca os callee-saved da thread atual como roots (mesmo critério do
        // scan: geração ≠ 0). Um false positive só mantém um slot vivo um ciclo
        // a mais — nunca corrompe.
        for &reg in saved.iter() {
            if reg != 0 && (reg >> HANDLE_GEN_SHIFT) & 0xFFFF != 0 {
                visit(reg);
            }
        }
        unsafe { scan_range(&mut *visit, rsp, stack_high) };

        // 2. Suspende e varre stack de cada thread RTS registrada. Necessário
        //    porque sem isso os handles vivos em workers eram swept e segfault
        //    sob carga (>16 conn http_server).
        #[cfg(target_os = "windows")]
        unsafe {
            mark_other_threads_windows(&mut *visit);
        }

        // 3. Globals top-level com handles (issue #407, epic #419).
        unsafe { mark_global_roots(&mut *visit) };
    }

    #[cfg(not(target_arch = "x86_64"))]
    let _ = visit;
}

#[cfg(target_arch = "x86_64")]
unsafe fn mark_global_roots(visit: &mut dyn FnMut(u64)) {
    let mut total = 0usize;
    let mut marked = 0usize;
    global_roots::for_each(|addr| {
        total += 1;
        // SAFETY: codegen só registra endereços de data symbols vivos enquanto
        // o JITModule existe (toda a vida do processo no `rts run`).
        let candidate = unsafe { *(addr as *const u64) };
        if candidate != 0 {
            let generation = (candidate >> HANDLE_GEN_SHIFT) & 0xFFFF;
            if generation != 0 {
                visit(candidate);
                marked += 1;
            }
        }
    });
    if debug::is_enabled() {
        eprintln!("[gc]   mark_global_roots globals={total} marks={marked}");
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn scan_range(visit: &mut dyn FnMut(u64), low: usize, high: usize) {
    if high <= low {
        return;
    }
    let mut addr = low;
    let mut marked = 0usize;
    while addr + 8 <= high {
        let candidate = unsafe { *(addr as *const u64) };
        if candidate != 0 {
            let generation = (candidate >> HANDLE_GEN_SHIFT) & 0xFFFF;
            if generation != 0 {
                visit(candidate);
                marked += 1;
            }
        }
        addr += 8;
    }
    if debug::is_enabled() {
        eprintln!(
            "[gc]   scan_range low={low:#x} high={high:#x} qwords={} marks={marked}",
            (high - low) / 8
        );
    }
}

/// Read every plausible handle word in `[low, high)` into `out`. A PURE memory
/// read — it takes NO lock and calls NO visitor, which is what makes it safe to
/// run while another thread is SUSPENDED (see [`mark_other_threads_windows`]).
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
unsafe fn collect_range(out: &mut Vec<u64>, low: usize, high: usize) {
    if high <= low {
        return;
    }
    let mut addr = low;
    while addr + 8 <= high {
        let candidate = unsafe { *(addr as *const u64) };
        if candidate != 0 {
            let generation = (candidate >> HANDLE_GEN_SHIFT) & 0xFFFF;
            if generation != 0 {
                out.push(candidate);
            }
        }
        addr += 8;
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
unsafe fn mark_other_threads_windows(visit: &mut dyn FnMut(u64)) {
    use thread_registry::{ThreadInfo, with_other_threads};

    // Coletamos primeiro pra liberar o lock do registry antes de suspender
    // threads (suspender com lock segurado = deadlock se o GC dispara dentro de
    // spawn).
    let mut others: Vec<ThreadInfo> = Vec::new();
    with_other_threads(|t| others.push(*t));

    // DEADLOCK RULE: while a thread is SUSPENDED we may only do lock-free work.
    // `visit` is the collector's `mark_handle`, which takes a HandleTable SHARD
    // LOCK. Suspending a thread that happens to be inside `alloc_entry`/`with_entry`
    // (holding that very shard's mutex) and then marking from this thread blocks
    // forever on a lock whose owner can never run — an unrecoverable deadlock with
    // every thread alive and CPU at zero. It was observed wedging the unit-test
    // binary. So: while suspended we only COLLECT candidate words (pure memory
    // reads, no locks); we RESUME first, and only then hand them to `visit`.
    //
    // Marking after the resume is sound because the scanner is CONSERVATIVE: a
    // word read from the frozen stack stays a valid mark candidate afterwards
    // (marking an already-dead candidate is filtered by generation+slot, and a
    // handle the thread drops after resuming is merely retained one extra cycle —
    // conservative, never a premature free).
    let mut candidates: Vec<u64> = Vec::new();
    for t in others {
        if t.handle == 0 {
            continue;
        }
        candidates.clear();
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
                if debug::is_enabled() {
                    eprintln!(
                        "[gc] scan OTHER tid={} rsp={rsp:#x} stack_high={:#x}",
                        t.thread_id, t.stack_high
                    );
                }
                collect_range(&mut candidates, rsp, t.stack_high);
                // Também coleta handles em registers callee-saved (RBX, RBP, RDI,
                // RSI, R12-R15) que podem segurar values vivos.
                for reg in [
                    ctx.Rbx, ctx.Rbp, ctx.Rdi, ctx.Rsi, ctx.R12, ctx.R13, ctx.R14, ctx.R15,
                ] {
                    let candidate = reg as u64;
                    if candidate != 0 {
                        let generation = (candidate >> HANDLE_GEN_SHIFT) & 0xFFFF;
                        if generation != 0 {
                            candidates.push(candidate);
                        }
                    }
                }
            }
            // RESUME BEFORE MARKING — `visit` takes shard locks (see above).
            ResumeThread(t.handle);
        }
        for &c in &candidates {
            visit(c);
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const CONTEXT_AMD64: u32 = 0x00100000;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const CONTEXT_CONTROL: u32 = CONTEXT_AMD64 | 0x1;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const CONTEXT_INTEGER: u32 = CONTEXT_AMD64 | 0x2;

// Layout de CONTEXT (Windows x64). Precisa estar 16-byte aligned. Apenas os
// campos que usamos (Rsp + callee-saved) têm nomes precisos; resto é padding.
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
#[repr(C, align(16))]
#[allow(non_snake_case)]
struct CONTEXT {
    P1Home: u64,
    P2Home: u64,
    P3Home: u64,
    P4Home: u64,
    P5Home: u64,
    P6Home: u64,
    ContextFlags: u32,
    MxCsr: u32,
    SegCs: u16,
    SegDs: u16,
    SegEs: u16,
    SegFs: u16,
    SegGs: u16,
    SegSs: u16,
    EFlags: u32,
    Dr0: u64,
    Dr1: u64,
    Dr2: u64,
    Dr3: u64,
    Dr6: u64,
    Dr7: u64,
    Rax: u64,
    Rcx: u64,
    Rdx: u64,
    Rbx: u64,
    Rsp: u64,
    Rbp: u64,
    Rsi: u64,
    Rdi: u64,
    R8: u64,
    R9: u64,
    R10: u64,
    R11: u64,
    R12: u64,
    R13: u64,
    R14: u64,
    R15: u64,
    Rip: u64,
    // Resto (FloatSave, vector regs, debug regs) — padding até 1232 bytes.
    _pad: [u8; 1232 - 0x100],
}

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
unsafe extern "system" {
    fn SuspendThread(h: usize) -> u32;
    fn ResumeThread(h: usize) -> u32;
    fn GetThreadContext(h: usize, ctx: *mut CONTEXT) -> i32;
}

/// Endereço alto (exclusivo) da stack da thread atual. Win32
/// `GetCurrentThreadStackLimits` (oficial, sempre coerente com RSP — não usar
/// `gs:[0x10]`/TIB.StackBase que às vezes < RSP). Linux via `/proc/self/maps`.
#[cfg(target_arch = "x86_64")]
fn stack_high_addr() -> usize {
    #[cfg(target_os = "windows")]
    {
        unsafe extern "system" {
            fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
        }
        let mut low: usize = 0;
        let mut high: usize = 0;
        unsafe {
            GetCurrentThreadStackLimits(&mut low, &mut high);
        }
        high
    }

    #[cfg(target_os = "linux")]
    {
        let mut rsp: usize;
        unsafe {
            std::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, readonly));
        }
        if let Ok(maps) = std::fs::read_to_string("/proc/self/maps") {
            for line in maps.lines() {
                if let Some((range, _)) = line.split_once(' ') {
                    if let Some((s, e)) = range.split_once('-') {
                        if let (Ok(start), Ok(end)) =
                            (usize::from_str_radix(s, 16), usize::from_str_radix(e, 16))
                        {
                            if rsp >= start && rsp < end {
                                return end;
                            }
                        }
                    }
                }
            }
        }
        rsp + 64 * 1024
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let mut rsp: usize;
        unsafe {
            std::arch::asm!("mov {}, rsp", out(reg) rsp, options(nostack, readonly));
        }
        rsp + 64 * 1024
    }
}
