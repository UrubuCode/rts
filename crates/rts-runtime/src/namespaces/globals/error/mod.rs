//! Error class family (Error/TypeError/RangeError/ReferenceError/SyntaxError/
//! URIError/EvalError/AggregateError).
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Todos os membros são
//! `external` — os externs `__RTS_FN_GL_*ERROR*` vivem em `instance.rs`/`rt.rs`
//! intactos; aqui só montamos os 8 `register_*_class_spec`. Os membros
//! message/name/toString/cause compartilham os mesmos externs
//! `__RTS_FN_GL_ERROR_*` entre todas as classes (via `symbol` explicito).

pub mod instance;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro de classe global `external` (fn_ptr nulo — o extern vive em
/// `instance.rs`/`rt.rs`). Espelha o helper hand-written do `boolean/mod.rs`,
/// adaptado pra membros sem `fn_ptr` próprio.
fn m(name: &str, kind: MemberKind, sig: Sig, symbol: &str, ts: &str) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a classe global `Error` (Fase 2 — hand-written, sem macro).
pub fn register_class_spec(e: &mut Engine) {
    e.class("Error")
        .doc("Error.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NEW",
            "new Error(message?: string, options?: object): Error",
        ))
        .member(m(
            "message",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_MESSAGE",
            "message: string",
        ))
        .member(m(
            "name",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NAME",
            "name: string",
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_TO_STRING",
            "toString(): string",
        ))
        .member(m(
            "cause",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_CAUSE",
            "cause: any",
        ))
        .member(m(
            "captureStackTrace",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Void),
            "__RTS_FN_GL_ERROR_CAPTURE_STACK_TRACE",
            "captureStackTrace(target: object, ctor?: Function): void",
        ))
        .done();
}

/// Registra a classe global `TypeError`.
pub fn register_type_error_class_spec(e: &mut Engine) {
    e.class("TypeError")
        .doc("TypeError.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_TYPE_ERROR_NEW",
            "new TypeError(message?: string, options?: object): TypeError",
        ))
        .member(m(
            "message",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_MESSAGE",
            "message: string",
        ))
        .member(m(
            "name",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NAME",
            "name: string",
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_TO_STRING",
            "toString(): string",
        ))
        .member(m(
            "cause",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_CAUSE",
            "cause: any",
        ))
        .done();
}

/// Registra a classe global `RangeError`.
pub fn register_range_error_class_spec(e: &mut Engine) {
    e.class("RangeError")
        .doc("RangeError.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_RANGE_ERROR_NEW",
            "new RangeError(message?: string, options?: object): RangeError",
        ))
        .member(m(
            "message",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_MESSAGE",
            "message: string",
        ))
        .member(m(
            "name",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NAME",
            "name: string",
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_TO_STRING",
            "toString(): string",
        ))
        .member(m(
            "cause",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_CAUSE",
            "cause: any",
        ))
        .done();
}

/// Registra a classe global `ReferenceError`.
pub fn register_ref_error_class_spec(e: &mut Engine) {
    e.class("ReferenceError")
        .doc("ReferenceError.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_REF_ERROR_NEW",
            "new ReferenceError(message?: string, options?: object): ReferenceError",
        ))
        .member(m(
            "message",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_MESSAGE",
            "message: string",
        ))
        .member(m(
            "name",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NAME",
            "name: string",
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_TO_STRING",
            "toString(): string",
        ))
        .member(m(
            "cause",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_CAUSE",
            "cause: any",
        ))
        .done();
}

/// Registra a classe global `SyntaxError`.
pub fn register_syntax_error_class_spec(e: &mut Engine) {
    e.class("SyntaxError")
        .doc("SyntaxError.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_SYNTAX_ERROR_NEW",
            "new SyntaxError(message?: string, options?: object): SyntaxError",
        ))
        .member(m(
            "message",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_MESSAGE",
            "message: string",
        ))
        .member(m(
            "name",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NAME",
            "name: string",
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_TO_STRING",
            "toString(): string",
        ))
        .member(m(
            "cause",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_CAUSE",
            "cause: any",
        ))
        .done();
}

/// Registra a classe global `URIError`.
pub fn register_uri_error_class_spec(e: &mut Engine) {
    e.class("URIError")
        .doc("URIError.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_URI_ERROR_NEW",
            "new URIError(message?: string, options?: object): URIError",
        ))
        .member(m(
            "message",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_MESSAGE",
            "message: string",
        ))
        .member(m(
            "name",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NAME",
            "name: string",
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_TO_STRING",
            "toString(): string",
        ))
        .member(m(
            "cause",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_CAUSE",
            "cause: any",
        ))
        .done();
}

/// Registra a classe global `EvalError`.
pub fn register_eval_error_class_spec(e: &mut Engine) {
    e.class("EvalError")
        .doc("EvalError.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_EVAL_ERROR_NEW",
            "new EvalError(message?: string, options?: object): EvalError",
        ))
        .member(m(
            "message",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_MESSAGE",
            "message: string",
        ))
        .member(m(
            "name",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NAME",
            "name: string",
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_TO_STRING",
            "toString(): string",
        ))
        .member(m(
            "cause",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_CAUSE",
            "cause: any",
        ))
        .done();
}

/// Registra a classe global `AggregateError` — ctor(errors, message); tem
/// `.errors`, sem `.cause`.
pub fn register_aggregate_error_class_spec(e: &mut Engine) {
    e.class("AggregateError")
        .doc("AggregateError — ctor(errors, message); tem `.errors`, sem `.cause`.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_AGGREGATE_ERROR_NEW",
            "new AggregateError(errors: any[], message?: string): AggregateError",
        ))
        .member(m(
            "message",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_MESSAGE",
            "message: string",
        ))
        .member(m(
            "name",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_NAME",
            "name: string",
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_ERROR_TO_STRING",
            "toString(): string",
        ))
        .member(m(
            "errors",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_AGGREGATE_ERROR_ERRORS",
            "errors: any[]",
        ))
        .done();
}
