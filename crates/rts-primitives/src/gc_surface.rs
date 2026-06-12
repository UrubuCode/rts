//! Declarações dos símbolos `#[no_mangle] extern "C"` do collector (vivem no
//! `rts-runtime`/`rts-std`) chamados pelos primordiais. Resolvem por LINK (JIT
//! registra; AOT pelo staticlib do rts-runtime) — não migram (carregam estado/
//! tipos backend: GC string pool, etc.). `safe fn` (edition 2024): as definições
//! são `extern "C"` seguras de chamar.
unsafe extern "C" {
    /// Aloca uma string GC a partir de bytes UTF-8, retorna handle.
    pub safe fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    /// Coage qualquer handle pra sua string-handle JS.
    pub safe fn __RTS_FN_RT_TO_STRING_HANDLE(handle: u64) -> u64;
}
