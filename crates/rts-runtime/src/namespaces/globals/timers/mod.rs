//! `timers` global — setTimeout / setInterval / setImmediate family. Migrado
//! ao modelo `#[rts_namespace]` (stage 2c) via membros `external`: os externs
//! `__RTS_FN_GL_TIMERS_*` ficam em `instance.rs` intactos; o macro deriva só o
//! `SPEC`.

pub mod instance;

#[allow(unused_imports)]
use rts_engine::abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;

/// setTimeout / clearTimeout / setInterval / clearInterval / setImmediate / clearImmediate.
#[rts_namespace(timers, sym = "GL_TIMERS")]
impl TimersNs {
    /// Chama callback(0) após delay_ms milissegundos. Retorna timer handle.
    #[rts_fn(
        external,
        name = "setTimeout",
        symbol = "__RTS_FN_GL_TIMERS_SET_TIMEOUT",
        ts = "setTimeout(callback: () => void, ms: number): number"
    )]
    pub fn set_timeout(_callback: U64, _ms: I64) -> Handle {
        unreachable!()
    }

    /// Cancela um timer criado por setTimeout.
    #[rts_fn(
        external,
        name = "clearTimeout",
        symbol = "__RTS_FN_GL_TIMERS_CLEAR_TIMEOUT",
        ts = "clearTimeout(handle: number): void"
    )]
    pub fn clear_timeout(_handle: Handle) {
        unreachable!()
    }

    /// Chama callback(0) repetidamente a cada interval_ms. Retorna timer handle.
    #[rts_fn(
        external,
        name = "setInterval",
        symbol = "__RTS_FN_GL_TIMERS_SET_INTERVAL",
        ts = "setInterval(callback: () => void, ms: number): number"
    )]
    pub fn set_interval(_callback: U64, _ms: I64) -> Handle {
        unreachable!()
    }

    /// Para um intervalo criado por setInterval.
    #[rts_fn(
        external,
        name = "clearInterval",
        symbol = "__RTS_FN_GL_TIMERS_CLEAR_INTERVAL",
        ts = "clearInterval(handle: number): void"
    )]
    pub fn clear_interval(_handle: Handle) {
        unreachable!()
    }

    /// Chama callback(0) o mais rápido possível (delay=0). Retorna timer handle.
    #[rts_fn(
        external,
        name = "setImmediate",
        symbol = "__RTS_FN_GL_TIMERS_SET_IMMEDIATE",
        ts = "setImmediate(callback: () => void): number"
    )]
    pub fn set_immediate(_callback: U64) -> Handle {
        unreachable!()
    }

    /// Cancela um setImmediate pendente.
    #[rts_fn(
        external,
        name = "clearImmediate",
        symbol = "__RTS_FN_GL_TIMERS_CLEAR_IMMEDIATE",
        ts = "clearImmediate(handle: number): void"
    )]
    pub fn clear_immediate(_handle: Handle) {
        unreachable!()
    }
}
