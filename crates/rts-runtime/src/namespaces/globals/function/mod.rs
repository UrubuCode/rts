//! Function — classe global do tipo primitivo JS Function (#359).
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Todos os membros sao
//! `external` — os externs `__RTS_FN_GL_FUNCTION_*` vivem em `ops.rs`/`props.rs`;
//! aqui só fica o `register_function_class_spec()` escrito à mão.
//!
//! Cobre `.call`, `.apply`, `.bind`, `.name`, `.length`, `.prototype` e o
//! constructor `new Function("a", "b", "return a+b")` via runtime.eval.
//!
//! Limitacoes vs Node:
//! - `.toString()` retorna `"function <name>() { [native code] }"` (RTS nao
//!   preserva source de fns declaradas estaticamente, exceto as criadas
//!   via `new Function`).
//! - `.prototype` nao existe (RTS separa classes de functions).
//! - `arguments` object nao existe (use rest params).
//! - `this` em fn declarations nao-arrow chamadas via `.call(thisArg)`:
//!   thisArg eh ignorado se a fn original nao for method de classe (RTS
//!   fns nao tem slot reservado pra this implicito).

pub mod ops;
pub mod props;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

// ── Builder hand-written da classe global `Function` ───────────────────────────

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a classe global `Function` no motor (hand-written, sem macro).
pub fn register_function_class_spec(e: &mut Engine) {
    e.class("Function")
        .doc(
            "Built-in Function class (#359). Todos os membros sao `external` — os externs \
             `__RTS_FN_GL_FUNCTION_*` vivem em `ops.rs`/`props.rs`; aqui o macro só deriva \
             o `FUNCTION_CLASS_SPEC` (stage 5, substitui o antigo `abi.rs`).",
        )
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FUNCTION_NEW",
            "constructor(...args: string[]): Function",
            "new Function(...args) — via runtime.eval.",
            core::ptr::null::<u8>(),
            false,
        ))
        .member(m(
            "call",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::I64, AbiType::Handle],
                AbiType::I64,
            ),
            "__RTS_FN_GL_FUNCTION_CALL",
            "call(thisArg: any, ...args: any[]): any",
            "fn.call(thisArg, ...args)",
            core::ptr::null::<u8>(),
            false,
        ))
        .member(m(
            "apply",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::I64, AbiType::Handle],
                AbiType::I64,
            ),
            "__RTS_FN_GL_FUNCTION_APPLY",
            "apply(thisArg: any, args: any[]): any",
            "fn.apply(thisArg, args)",
            core::ptr::null::<u8>(),
            false,
        ))
        .member(m(
            "bind",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::I64, AbiType::Handle],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_FUNCTION_BIND",
            "bind(thisArg: any, ...args: any[]): Function",
            "fn.bind(thisArg, ...args)",
            core::ptr::null::<u8>(),
            false,
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FUNCTION_TO_STRING",
            "toString(): string",
            "fn.toString()",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "name",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FUNCTION_NAME",
            "readonly name: string",
            "fn.name",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "length",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_FUNCTION_LENGTH",
            "readonly length: number",
            "fn.length",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "prototype",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FUNCTION_PROTOTYPE_GET",
            "prototype: any",
            "fn.prototype",
            core::ptr::null::<u8>(),
            false,
        ))
        .done();
}
