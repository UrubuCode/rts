//! Declarações dos símbolos `#[unsafe(no_mangle)] extern "C"` do collector do
//! `rts-runtime` que módulos backend (rts-std) chamam — o slot de erro
//! thread-local (`gc::error`) e o resume da máquina de estados async
//! (`gc::generator`). Esses ficam no runtime (carregam estado/tipos backend);
//! o rts-std os resolve por link.
//!
//! `safe fn` (edition 2024): as definições no runtime são fns Rust `extern "C"`
//! seguras de chamar — preserva call-sites sem `unsafe`.

unsafe extern "C" {
    /// Lê o handle do erro pendente (0 = nenhum). (`gc::error`)
    pub safe fn __RTS_FN_RT_ERROR_GET() -> u64;
    /// Lê o handle do stack do erro pendente (0 = nenhum). (`gc::error`)
    pub safe fn __RTS_FN_RT_ERROR_GET_STACK() -> u64;
    /// Limpa o erro pendente + libera o stack capturado. (`gc::error`)
    pub safe fn __RTS_FN_RT_ERROR_CLEAR();
    /// Define o erro pendente (valor lançado) + captura stack. (`gc::error`)
    pub safe fn __RTS_FN_RT_ERROR_SET(handle: u64);
    /// Injeta valor/erro numa async SM e roda o próximo passo. (`gc::generator`)
    pub safe fn __RTS_FN_RT_ASYNC_SM_RESUME(h: u64, value: i64, rejected: i64);
}
