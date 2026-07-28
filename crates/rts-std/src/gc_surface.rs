//! Declarações dos símbolos `#[unsafe(no_mangle)] extern "C"` do collector do
//! `rts-runtime` que módulos backend (rts-std) chamam — o slot de erro
//! thread-local (`gc::error`) e o resume da máquina de estados async
//! (`gc::generator`). Esses ficam no runtime (carregam estado/tipos backend);
//! o rts-std os resolve por link.
//!
//! `safe fn` (edition 2024): as definições no runtime são fns Rust `extern "C"`
//! seguras de chamar — preserva call-sites sem `unsafe`.
//!
//! `__RTS_FN_RT_ERROR_SET` e `__RTS_FN_NS_GC_GEN_SM_DRAIN` moraram aqui antes;
//! agora vêm de `rts_engine::gc_surface` (site único de declaração — ver lá a
//! justificativa de layering). Re-exportados abaixo p/ os call-sites locais
//! (`crate::gc_surface::...`) continuarem resolvendo sem edição.
pub use rts_engine::gc_surface::{
    __RTS_FN_GL_FUNCTION_CALL, __RTS_FN_GL_PROMISE_REJECT, __RTS_FN_GL_PROMISE_RESOLVE,
    __RTS_FN_NS_GC_GEN_SM_DRAIN, __RTS_FN_NS_GC_STRING_CONCAT, __RTS_FN_NS_GC_STRING_NEW,
    __RTS_FN_RT_ERROR_SET, __RTS_FN_RT_INVOKE_AUTO, __RTS_FN_RT_TO_STRING_HANDLE,
};

unsafe extern "C" {
    /// Lê o handle do erro pendente (0 = nenhum). (`gc::error`)
    pub safe fn __RTS_FN_RT_ERROR_GET() -> u64;
    /// Lê o handle do stack do erro pendente (0 = nenhum). (`gc::error`)
    pub safe fn __RTS_FN_RT_ERROR_GET_STACK() -> u64;
    /// Limpa o erro pendente + libera o stack capturado. (`gc::error`)
    pub safe fn __RTS_FN_RT_ERROR_CLEAR();
    /// Injeta valor/erro numa async SM e roda o próximo passo. (`gc::generator`)
    pub safe fn __RTS_FN_RT_ASYNC_SM_RESUME(h: u64, value: i64, rejected: i64);
    /// Truthiness JS de um valor i64/handle (0 = falsy). (`gc::string_pool`)
    pub safe fn __RTS_FN_RT_TRUTHY(value: i64) -> i64;
    /// (callback-from-runtime bridge) Invoca uma FUNCAO-VALOR JS (palavra PolyValue
    /// TAG_FUNCTION) com 4 args PolyValue + rest, retornando uma palavra PolyValue.
    /// Codegen-owned (`value::funcops`), instalado no JIT symbol table. Permite a um
    /// backend (EventEmitter/Proxy/…) chamar de volta um callback armazenado, sem
    /// transmutar a palavra como ponteiro de codigo (que crashava). JIT-only por ora
    /// (o simbolo nao esta no archive AOT — async/paralelo real e' redesign limpo).
    pub safe fn __rtsadp_fn_invoke(
        fn_word: u64,
        a0: u64,
        a1: u64,
        a2: u64,
        a3: u64,
        rest: u64,
    ) -> u64;
}
