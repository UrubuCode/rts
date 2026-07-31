//! Registro global de threads RTS para que o GC consiga varrer a stack
//! de todas elas durante `mark_stack_roots()`.
//!
//! Sem este registro, o stack scanner so' enxergava a thread que disparou
//! o tick (chamou `alloc_entry` que bateu o intervalo) — handles vivos em
//! qualquer outra thread eram marcados como nao-alcancaveis e o sweep
//! os liberava, causando segfaults sob carga concorrente.
//!
//! API:
//! - `register_current()` — chama no inicio de cada thread RTS spawnada.
//! - `unregister_current()` — chama no fim (cleanup).
//! - `with_other_threads(|info| ...)` — itera sobre threads vivas
//!   diferentes da atual. Cada `info` traz `HANDLE` Win32 (para
//!   `SuspendThread`/`GetThreadContext`) e `stack_high`.
//!
//! Plataforma: Windows-only por enquanto (usa `OpenThread` + duplicar
//! HANDLE pseudo `GetCurrentThread`). Em outros OSes vira no-op — main
//! thread e' a unica varrida, igual antes.

use std::sync::Mutex;
use std::sync::OnceLock;

/// Identificador de thread + stack bounds. Em Windows, `handle` e' um
/// `HANDLE` Win32 valido para uso com `SuspendThread`/`GetThreadContext`/
/// `ResumeThread`. `stack_high` e' o endereco exclusivo do topo da stack
/// (alto), capturado quando a thread se registrou.
#[derive(Debug, Clone, Copy)]
pub struct ThreadInfo {
    pub thread_id: u64,
    /// Win32 HANDLE (na verdade `*mut c_void`) ou 0 em outros OSes.
    pub handle: usize,
    pub stack_high: usize,
}

// Safety: HANDLE e' um pseudo-pointer mas nao e' dereferenciado pelo Rust;
// e' so' usado pelas APIs Win32. Em todos os outros OSes esse campo e' 0.
unsafe impl Send for ThreadInfo {}

fn registry() -> &'static Mutex<Vec<ThreadInfo>> {
    static REG: OnceLock<Mutex<Vec<ThreadInfo>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(Vec::new()))
}

/// Registra a thread atual no registry global. Idempotente — re-registro
/// substitui a entry anterior pra mesma thread_id.
pub fn register_current() {
    let info = current_thread_info();
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    guard.retain(|t| t.thread_id != info.thread_id);
    guard.push(info);
}

/// Remove a thread atual do registry. Chamado no fim do worker.
pub fn unregister_current() {
    let tid = current_thread_id();
    let mut guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(pos) = guard.iter().position(|t| t.thread_id == tid) {
        // Fecha o HANDLE duplicado para nao vazar (Windows).
        #[cfg(target_os = "windows")]
        {
            let h = guard[pos].handle;
            if h != 0 {
                unsafe {
                    CloseHandle(h);
                }
            }
        }
        guard.remove(pos);
    }
}

/// Itera sobre threads registradas que NAO sao a atual. Util para o GC
/// suspender e varrer cada uma sem incluir a si proprio.
pub fn with_other_threads<F: FnMut(&ThreadInfo)>(mut f: F) {
    let me = current_thread_id();
    let guard = registry().lock().unwrap_or_else(|e| e.into_inner());
    for t in guard.iter() {
        if t.thread_id != me {
            f(t);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn GetCurrentThread() -> usize;
    fn GetCurrentThreadId() -> u32;
    fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    fn DuplicateHandle(
        src_proc: usize,
        src_handle: usize,
        dst_proc: usize,
        dst_handle: *mut usize,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn GetCurrentProcess() -> usize;
    fn CloseHandle(h: usize) -> i32;
}

#[cfg(target_os = "windows")]
const DUPLICATE_SAME_ACCESS: u32 = 0x2;

fn current_thread_id() -> u64 {
    #[cfg(target_os = "windows")]
    unsafe {
        GetCurrentThreadId() as u64
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Fallback: usa endereco da thread-local var como ID estavel.
        thread_local! { static MARKER: u8 = const { 0 }; }
        MARKER.with(|m| m as *const _ as u64)
    }
}

fn current_thread_info() -> ThreadInfo {
    #[cfg(target_os = "windows")]
    {
        let mut low: usize = 0;
        let mut high: usize = 0;
        unsafe {
            GetCurrentThreadStackLimits(&mut low, &mut high);
        }
        // Pseudo-handle GetCurrentThread() so' funciona na thread dona.
        // Pra usar de outra thread precisamos duplicar para um real.
        let mut real_handle: usize = 0;
        unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                GetCurrentThread(),
                GetCurrentProcess(),
                &mut real_handle,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            );
        }
        ThreadInfo {
            thread_id: current_thread_id(),
            handle: real_handle,
            stack_high: high,
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        ThreadInfo {
            thread_id: current_thread_id(),
            handle: 0,
            stack_high: 0,
        }
    }
}
