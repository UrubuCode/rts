//! `timers` global — setTimeout / setInterval / setImmediate family. Migrado do
//! `#[rts_namespace]` pro modelo builder hand-written do `rts-engine` (rumo à
//! remoção da `rts-macro`). Todos os membros são `external`: os externs
//! `__RTS_FN_GL_TIMERS_*` ficam em `instance.rs` intactos; este arquivo só
//! registra a SPEC referenciando esses símbolos (`fn_ptr` null).

pub mod instance;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro `external`: a SPEC referencia o `symbol` cujo extern vive em
/// `instance.rs`. `fn_ptr` é null (o submódulo dono provê o ponteiro real).
fn ext(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `timers` no motor (hand-written, sem macro). Todos os
/// membros são `external` — os externs pertencem a `instance.rs`.
pub fn register(e: &mut Engine) {
    e.ns("timers")
        .doc("setTimeout / clearTimeout / setInterval / clearInterval / setImmediate / clearImmediate.")
        .member(ext(
            "setTimeout",
            "__RTS_FN_GL_TIMERS_SET_TIMEOUT",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Handle),
            "setTimeout(callback: () => void, ms: number): number",
            "Chama callback(0) após delay_ms milissegundos. Retorna timer handle.",
        ))
        .member(ext(
            "clearTimeout",
            "__RTS_FN_GL_TIMERS_CLEAR_TIMEOUT",
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "clearTimeout(handle: number): void",
            "Cancela um timer criado por setTimeout.",
        ))
        .member(ext(
            "setInterval",
            "__RTS_FN_GL_TIMERS_SET_INTERVAL",
            Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Handle),
            "setInterval(callback: () => void, ms: number): number",
            "Chama callback(0) repetidamente a cada interval_ms. Retorna timer handle.",
        ))
        .member(ext(
            "clearInterval",
            "__RTS_FN_GL_TIMERS_CLEAR_INTERVAL",
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "clearInterval(handle: number): void",
            "Para um intervalo criado por setInterval.",
        ))
        .member(ext(
            "setImmediate",
            "__RTS_FN_GL_TIMERS_SET_IMMEDIATE",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "setImmediate(callback: () => void): number",
            "Chama callback(0) o mais rápido possível (delay=0). Retorna timer handle.",
        ))
        .member(ext(
            "clearImmediate",
            "__RTS_FN_GL_TIMERS_CLEAR_IMMEDIATE",
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "clearImmediate(handle: number): void",
            "Cancela um setImmediate pendente.",
        ))
        .done();
}
