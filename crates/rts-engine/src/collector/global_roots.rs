//! Registry de globals top-level com handles para o GC marcar como roots.
//!
//! Issue #407 — globals declarados via `let buffer = "..."` no top-level
//! são alocadas como data symbols (8 bytes) em `compile_program`. O
//! handle armazenado nessa global não está em nenhuma stack — o
//! `mark_stack_roots()` conservativo não enxerga, e o sweep eventualmente
//! coleta o handle ativo.
//!
//! Solução: codegen registra cada global de tipo `Handle` aqui após
//! `module.finalize_definitions()` (quando o endereço real do data symbol
//! é conhecido). O GC, no `mark_stack_roots()`, lê cada endereço como
//! `*const u64` e marca o handle.
//!
//! Thread-safety: registry é populado uma vez no startup, depois só leitura.
//! `RwLock` cobre a fase de bootstrap; reads no GC são paralelas.

use std::sync::OnceLock;
use std::sync::RwLock;

fn registry() -> &'static RwLock<Vec<usize>> {
    static R: OnceLock<RwLock<Vec<usize>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(Vec::new()))
}

/// Adiciona um endereço ao registry. `addr` deve apontar para um
/// `u64` que armazena um handle GC. Tipicamente chamado pelo
/// codegen JIT após `finalize_definitions()` para cada data symbol
/// de global com `ValTy::Handle`.
pub fn add(addr: usize) {
    if addr == 0 {
        return;
    }
    let mut guard = registry().write().unwrap_or_else(|e| e.into_inner());
    if !guard.contains(&addr) {
        guard.push(addr);
    }
}

/// Itera sobre os endereços registrados. O GC chama no `mark_stack_roots`
/// e lê cada endereço como ponteiro pra u64, marca o handle.
pub fn for_each<F: FnMut(usize)>(mut f: F) {
    let guard = registry().read().unwrap_or_else(|e| e.into_inner());
    for addr in guard.iter() {
        f(*addr);
    }
}

/// Remove um endereço (útil em hot-reload onde o módulo JIT é recompilado).
pub fn remove(addr: usize) {
    let mut guard = registry().write().unwrap_or_else(|e| e.into_inner());
    guard.retain(|a| *a != addr);
}

/// Limpa todo o registry. Útil pra testes / hot-reload completo.
pub fn clear() {
    let mut guard = registry().write().unwrap_or_else(|e| e.into_inner());
    guard.clear();
}

/// Total de globals registradas. Diagnóstico.
pub fn len() -> usize {
    registry().read().unwrap_or_else(|e| e.into_inner()).len()
}

// ─── Registry de DataIds pendentes (compile-time) ────────────────────────
//
// Codegen acumula DataIds de globals com tipo Handle durante
// `compile_program`. Apos `finalize_definitions`, `jit.rs` chama
// `drain_pending_data_ids()` e resolve cada DataId pra endereco real
// via `module.get_finalized_data()`, registrando via `add(addr)`.

use std::sync::Mutex;

fn pending() -> &'static Mutex<Vec<u32>> {
    static P: OnceLock<Mutex<Vec<u32>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(Vec::new()))
}

/// Codegen chama isto durante `compile_program` para cada global de
/// tipo Handle declarada. Argumento e' o `DataId.as_u32()` —
/// resolveremos pra endereco apos finalize_definitions.
pub fn push_pending_data_id(data_id_raw: u32) {
    pending()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(data_id_raw);
}

/// JIT chama isto apos `finalize_definitions`. Devolve a lista
/// acumulada e limpa.
pub fn drain_pending_data_ids() -> Vec<u32> {
    let mut guard = pending().lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_iterate() {
        clear();
        add(0x1000);
        add(0x2000);
        add(0x1000); // duplicada — ignora
        let mut count = 0;
        for_each(|_| count += 1);
        assert_eq!(count, 2);
    }

    #[test]
    fn remove_works() {
        clear();
        add(0x3000);
        add(0x4000);
        remove(0x3000);
        let mut found = Vec::new();
        for_each(|a| found.push(a));
        assert_eq!(found, vec![0x4000]);
    }

    #[test]
    fn zero_addr_ignored() {
        clear();
        add(0);
        assert_eq!(len(), 0);
    }
}
