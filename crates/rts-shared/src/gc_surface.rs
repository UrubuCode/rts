//! Declarações dos símbolos `#[unsafe(no_mangle)] extern "C"` que vivem no
//! collector do `rts-runtime` (`string_pool`/`error`/`generator`) e são
//! chamados pela camada universal (`rts-shared`).
//!
//! Esses símbolos NÃO migram pro shared (carregam estado/tipos backend —
//! GC string pool, slot de erro thread-local, máquina de estados de generator).
//! O shared os resolve por link como `extern "C"` — o linker (JIT e AOT) acha a
//! definição no runtime/staticlib. (GOTCHA provado em `CONTINUE.md`: externs
//! `#[no_mangle]` sobrevivem ao AOT do staticlib do rts-runtime.)

// `safe fn` (edition 2024): as definições no runtime collector são funções Rust
// `extern "C"` seguras de chamar (sem invariantes de chamador) — declará-las
// `safe` aqui preserva a semântica original (call-sites sem `unsafe`).
unsafe extern "C" {
    /// Aloca uma string GC a partir de bytes UTF-8, retorna handle. (`gc::string_pool`)
    pub safe fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    /// Coage qualquer handle pra sua string-handle JS (`gc::string_pool`).
    pub safe fn __RTS_FN_RT_TO_STRING_HANDLE(handle: u64) -> u64;
    /// Define o erro pendente (valor lançado) + captura stack (`gc::error`).
    pub safe fn __RTS_FN_RT_ERROR_SET(handle: u64);
    /// Handle da Function nativa do iterator de Array (`gc::generator`).
    pub safe fn __RTS_FN_GL_ARRAY_ITERATOR_FN() -> u64;
    /// Drena a máquina de estados de um generator (`gc::generator`).
    pub safe fn __RTS_FN_NS_GC_GEN_SM_DRAIN(h: u64) -> u64;
}
