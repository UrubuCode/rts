//! Spec da classe primordial `Promise` (metadata pura — fn_ptr null, símbolos
//! `__RTS_FN_GL_PROMISE_*` / `__RTS_FN_NS_PROMISE_*`). A impl tokio fica em
//! `rts-std/globals/fetch`; aqui só a declaração de membros que o motor resolve
//! via Registry. Movida de `rts-std/globals/fetch/mod.rs` na Fase 2 (Promise é
//! primordial). Crate só depende de `rts-engine`.

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro hand-written (espelha `leak`/`leak_class` da macro). `fn_ptr` é null
/// para membros `external` (codegen resolve pelo `symbol`).
#[allow(clippy::too_many_arguments)]
fn m(name: &str, kind: MemberKind, sig: Sig, symbol: &str, ts: &str, doc: &str, pure: bool) -> Member {
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
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a classe global `Promise` no motor (hand-written, sem macro).
pub fn register_promise_class_spec(e: &mut Engine) {
    e.class("Promise")
        .doc("Promise síncrona — then/catch/finally executam imediatamente. await = resolve().")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_NEW",
            "new (executor: (resolve: (value: any) => void, reject: (reason: any) => void) => void): Promise<any>",
            "new Promise((resolve, reject) => ...) — JS spec ctor.",
            false,
        ))
        .member(m(
            "then",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_THEN",
            "then<T>(onFulfilled: (value: any) => T): Promise<T>",
            "promise.then(onFul) — chama onFul(value) ao resolve. Suporta PromiseAsync (#411) com encadeamento real.",
            false,
        ))
        .member(m(
            "catch",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_CATCH",
            "catch(onRejected: (err: any) => any): Promise<any>",
            "promise.catch(onRej) — captura rejection. Recovers se callback retornar valor.",
            false,
        ))
        .member(m(
            "finally",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_FINALLY",
            "finally(onFinally: () => void): Promise<any>",
            "promise.finally(fn) — chama fn() ao settle. Mantem state/value original.",
            false,
        ))
        .member(m(
            "resolve",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_PROMISE_RESOLVE",
            "static resolve<T>(value: T): Promise<T>",
            "Promise.resolve(v) — cria Promise já resolvida.",
            false,
        ))
        .member(m(
            "resolve",
            MemberKind::Function,
            Sig::new(Vec::new(), AbiType::I64),
            "__RTS_FN_GL_PROMISE_RESOLVE_EMPTY",
            "static resolve(): Promise<undefined>",
            "Promise.resolve() — cria Promise já resolvida com undefined.",
            false,
        ))
        .member(m(
            "reject",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_PROMISE_REJECT",
            "static reject<T>(reason?: any): Promise<T>",
            "Promise.reject(reason) — cria PromiseAsync rejected. Distingue de resolve para .catch/.any/.allSettled.",
            false,
        ))
        .member(m(
            "all",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_NS_PROMISE_ALL",
            "static all<T>(values: Promise<T>[]): Promise<T[]>",
            "Promise.all(promises) — aguarda todas; fail-fast em rejection.",
            false,
        ))
        .member(m(
            "race",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_NS_PROMISE_RACE",
            "static race<T>(values: Promise<T>[]): Promise<T>",
            "Promise.race(promises) — settle com o resultado da primeira.",
            false,
        ))
        .member(m(
            "any",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_NS_PROMISE_ANY",
            "static any<T>(values: Promise<T>[]): Promise<T>",
            "Promise.any(promises) — primeira a fulfill; reject so' se todas falharem.",
            false,
        ))
        .member(m(
            "allSettled",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_NS_PROMISE_ALL_SETTLED",
            "static allSettled<T>(values: Promise<T>[]): Promise<any[]>",
            "Promise.allSettled(promises) — aguarda todas; sempre resolve com Vec de descriptors.",
            false,
        ))
        .member(m(
            "try",
            MemberKind::Function,
            Sig::new(vec![AbiType::U64], AbiType::I64),
            "__RTS_FN_GL_PROMISE_TRY",
            "static try<T>(fn: () => T | Promise<T>): Promise<T>",
            "Promise.try(fn) — invoca fn() sincronamente; retorna Promise.resolve(retval) ou Promise.reject(err) se fn lancar.",
            false,
        ))
        .member(m(
            "withResolvers",
            MemberKind::Function,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_PROMISE_WITH_RESOLVERS",
            "static withResolvers<T>(): { promise: Promise<T>, resolve: (v: T) => void, reject: (e: any) => void }",
            "Promise.withResolvers() — ES2024: cria { promise, resolve, reject }. Util pra fora-do-construtor settle.",
            false,
        ))
        .done();
}
