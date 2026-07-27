//! `console` global namespace — variadic print methods. Migrado do
//! `#[rts_namespace]` pro modelo builder hand-written do `rts-engine` (rumo à
//! remoção da `rts-macro`). Todos os membros são `external`: os símbolos
//! apontam para `io.*` (codegen concatena os args variádicos antes de chamar);
//! `fn_ptr` é null (o namespace `io` provê o ponteiro real). O runtime override
//! side-table (`rt.rs`) fica intacto.
//!
//! Codegen ainda especializa `console.*` porque os métodos são variádicos
//! (nº arbitrário de args de qualquer tipo), o que não cabe no ABI fixo
//! `AbiType[]`; os `symbol` apontam para os alvos `io.*` reais.

pub mod rt;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro `external`: a SPEC referencia o `symbol` (de `io.*`); `fn_ptr` é null
/// (o namespace dono provê o ponteiro real no link JIT/AOT).
fn ext(name: &str, symbol: &str, ts: &str, doc: &str) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(vec![AbiType::StrPtr], AbiType::Void),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `console` no motor (hand-written, sem macro). Todos os
/// membros são `external` — os externs `io.*` pertencem ao namespace `io`.
pub fn register(e: &mut Engine) {
    e.ns("console")
        .doc("Global console object — variadic print to stdout/stderr.")
        .member(ext(
            "log",
            "__RTS_FN_NS_IO_PRINT",
            "log(...args: unknown[]): void",
            "Prints args separated by spaces to stdout.",
        ))
        .member(ext(
            "info",
            "__RTS_FN_NS_IO_PRINT",
            "info(...args: unknown[]): void",
            "Alias for console.log.",
        ))
        .member(ext(
            "debug",
            "__RTS_FN_NS_IO_PRINT",
            "debug(...args: unknown[]): void",
            "Alias for console.log.",
        ))
        .member(ext(
            "error",
            "__RTS_FN_NS_IO_EPRINT",
            "error(...args: unknown[]): void",
            "Prints args separated by spaces to stderr.",
        ))
        .member(ext(
            "warn",
            "__RTS_FN_NS_IO_EPRINT",
            "warn(...args: unknown[]): void",
            "Alias for console.error.",
        ))
        .member(ext(
            "assert",
            "__RTS_FN_NS_IO_EPRINT",
            "assert(cond: unknown, ...msg: unknown[]): void",
            "If cond is falsy, prints \"Assertion failed: <msg>\" to stderr; otherwise no-op. Does not throw.",
        ))
        .member(ext(
            "dir",
            "__RTS_FN_NS_IO_PRINT",
            "dir(arg: unknown): void",
            "Pretty-prints a single argument via INSPECT (alias of console.log for one arg).",
        ))
        .done();
}
