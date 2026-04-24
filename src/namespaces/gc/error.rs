//! Thread-local error slot para `try/catch/throw` do codegen.
//!
//! Modelo intencionalmente simples (fase 1 de #62): um slot global
//! thread-local guarda o handle de string da exception pendente, 0
//! quando nao ha. `throw` seta o slot; o inicio de cada `try` zera;
//! o final de cada statement em body de `try` checa o slot e pula
//! para `catch` se nao-zero.
//!
//! Nao ha unwind real — se o throw estiver dentro de uma funcao que
//! nao coopere (ou dentro de call extern nao-TS), o erro fica
//! pendente mas nao propaga. Suficiente para throw/catch em user
//! code tipico; insuficiente para exceptions cruzando FFI.
//!
//! Fase 2/3 quando/se necessario: cranelift `invoke` + SEH/DWARF.

use std::cell::Cell;

thread_local! {
    static ERROR_SLOT: Cell<u64> = const { Cell::new(0) };
}

/// Seta o handle de erro pendente. Usado por `throw`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ERROR_SET(handle: u64) {
    ERROR_SLOT.with(|s| s.set(handle));
}

/// Le o handle pendente. 0 significa sem erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ERROR_GET() -> u64 {
    ERROR_SLOT.with(|s| s.get())
}

/// Limpa o slot. Usado no catch apos ler o valor.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ERROR_CLEAR() {
    ERROR_SLOT.with(|s| s.set(0));
}
