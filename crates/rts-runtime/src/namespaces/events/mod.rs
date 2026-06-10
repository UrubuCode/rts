//! `events` namespace — EventEmitter primitivo handle-based.
//!
//! Listeners sao function pointers raw (`func_addr` em i64). Caller
//! (codegen) materializa o endereco da fn via `Expr::Ident` → `func_addr`
//! e passa pra `events.on`. `events.emit*` invoca o ponteiro via
//! `unsafe transmute` para a signature apropriada.
//!
//! `node:events` (#290) é wrapper TS sobre este namespace. Migrado do
//! `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use crate::namespaces::gc::handles::{
    alloc_entry, free_handle, with_entry, with_entry_mut, Entry, RtsEventsEmitter,
};

fn with_emitter<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&RtsEventsEmitter) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::RtsEventsEmitter(e)) => f(e.as_ref()),
        _ => default,
    })
}

fn with_emitter_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut RtsEventsEmitter) -> R,
{
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::RtsEventsEmitter(e)) => f(e.as_mut()),
        _ => default,
    })
}

/// Aloca um EventEmitter e retorna o handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMITTER_NEW() -> Handle {
    alloc_entry(Entry::RtsEventsEmitter(Box::new(
        RtsEventsEmitter::default(),
    )))
}

/// Libera o EventEmitter.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMITTER_FREE(h: Handle) {
    free_handle(h);
}

/// Registra um listener (function pointer raw) para o evento.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_ON(h: Handle, name_ptr: *const u8, name_len: i64, fn_ptr: U64) {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return,
    };
    let key = name.to_string();
    with_emitter_mut(h, (), |e| {
        e.listeners.entry(key).or_default().push(fn_ptr);
    });
}

/// Remove o primeiro listener com o ponteiro especificado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_OFF(h: Handle, name_ptr: *const u8, name_len: i64, fn_ptr: U64) {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return,
    };
    with_emitter_mut(h, (), |e| {
        if let Some(list) = e.listeners.get_mut(name) {
            if let Some(idx) = list.iter().position(|&p| p == fn_ptr) {
                list.remove(idx);
            }
        }
    });
}

/// Remove todos os listeners do evento.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_REMOVE_ALL_LISTENERS(h: Handle, name_ptr: *const u8, name_len: i64) {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return,
    };
    with_emitter_mut(h, (), |e| {
        e.listeners.remove(name);
    });
}

/// Numero de listeners registrados para o evento.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_LISTENER_COUNT(h: Handle, name_ptr: *const u8, name_len: i64) -> I64 {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return 0,
    };
    with_emitter(h, 0, |e| {
        e.listeners.get(name).map(|l| l.len() as i64).unwrap_or(0)
    })
}

/// Dispara `name` sem argumentos. Retorna 1 se havia listeners, 0 caso contrario.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMIT0(h: Handle, name_ptr: *const u8, name_len: i64) -> I64 {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return 0,
    };
    // Snapshot a lista antes de chamar — caller pode `off` durante o
    // dispatch sem invalidar o iterador.
    let snapshot: Vec<u64> = with_emitter(h, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if snapshot.is_empty() {
        return 0;
    }
    for fp in &snapshot {
        // SAFETY: caller contract — fp veio de `func_addr` de uma user fn
        // registrada com signature compativel `extern "C" fn()`.
        let f: extern "C" fn() = unsafe { std::mem::transmute(*fp as usize) };
        f();
    }
    1
}

/// Dispara `name` com 1 argumento i64. Sincrono sequencial Node-style — listeners chamados em ordem de registro na thread atual. Para dispatch paralelo fire-and-forget use `emit1_async`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMIT1(h: Handle, name_ptr: *const u8, name_len: i64, arg0: I64) -> I64 {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return 0,
    };
    let snapshot: Vec<u64> = with_emitter(h, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if snapshot.is_empty() {
        return 0;
    }
    for fp in &snapshot {
        // SAFETY: caller contract — fp aceita `extern "C" fn(i64)`.
        let f: extern "C" fn(i64) = unsafe { std::mem::transmute(*fp as usize) };
        f(arg0);
    }
    1
}

/// Dispara `name` sem args, fire-and-forget paralelo via tokio (cada listener vira `spawn_blocking`). ATENCAO: bench mostra que e' ~10× MAIS LENTO que `emit0` para listeners leves (atomic, memory ops, log). So' vale a pena quando cada listener faz trabalho pesado (>10µs cada): HTTP request, disk I/O, calculo numerico longo. Para tudo mais use `emit0` que e' sequencial mas drasticamente mais rapido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMIT0_ASYNC(h: Handle, name_ptr: *const u8, name_len: i64) -> I64 {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return 0,
    };
    let listeners: Vec<u64> = with_emitter(h, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if listeners.is_empty() {
        return 0;
    }
    let rt = crate::runtime::async_rt::handle();
    for fp in listeners {
        rt.spawn_blocking(move || {
            // SAFETY: caller contract — `extern "C" fn()` via `events.on`.
            let f: extern "C" fn() = unsafe { std::mem::transmute(fp as usize) };
            f();
        });
    }
    1
}

/// Variante async de `emit1`. Mesmo trade-off de `emit0_async`: usar so' quando listeners individuais sao pesados.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMIT1_ASYNC(h: Handle, name_ptr: *const u8, name_len: i64, arg0: I64) -> I64 {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return 0,
    };
    let listeners: Vec<u64> = with_emitter(h, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if listeners.is_empty() {
        return 0;
    }
    let rt = crate::runtime::async_rt::handle();
    for fp in listeners {
        rt.spawn_blocking(move || {
            // SAFETY: caller contract — `extern "C" fn(i64)`.
            let f: extern "C" fn(i64) = unsafe { std::mem::transmute(fp as usize) };
            f(arg0);
        });
    }
    1
}

/// Função `events.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `events` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("events")
        .doc("Sistema de eventos primitivo. Listeners sao function pointers raw, invocados via transmute para extern \"C\" fn.")
        .member(func(
            "emitter_new",
            "__RTS_FN_NS_EVENTS_EMITTER_NEW",
            Sig::new(Vec::new(), AbiType::Handle),
            "emitter_new(): number",
            "Aloca um EventEmitter e retorna o handle.",
            __RTS_FN_NS_EVENTS_EMITTER_NEW as *const u8,
        ))
        .member(func(
            "emitter_free",
            "__RTS_FN_NS_EVENTS_EMITTER_FREE",
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "emitter_free(h: number): void",
            "Libera o EventEmitter.",
            __RTS_FN_NS_EVENTS_EMITTER_FREE as *const u8,
        ))
        .member(func(
            "on",
            "__RTS_FN_NS_EVENTS_ON",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr, AbiType::U64], AbiType::Void),
            "on(h: number, name: string, fnPtr: number): void",
            "Registra um listener (function pointer raw) para o evento.",
            __RTS_FN_NS_EVENTS_ON as *const u8,
        ))
        .member(func(
            "off",
            "__RTS_FN_NS_EVENTS_OFF",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr, AbiType::U64], AbiType::Void),
            "off(h: number, name: string, fnPtr: number): void",
            "Remove o primeiro listener com o ponteiro especificado.",
            __RTS_FN_NS_EVENTS_OFF as *const u8,
        ))
        .member(func(
            "remove_all_listeners",
            "__RTS_FN_NS_EVENTS_REMOVE_ALL_LISTENERS",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "remove_all_listeners(h: number, name: string): void",
            "Remove todos os listeners do evento.",
            __RTS_FN_NS_EVENTS_REMOVE_ALL_LISTENERS as *const u8,
        ))
        .member(func(
            "listener_count",
            "__RTS_FN_NS_EVENTS_LISTENER_COUNT",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::I64),
            "listener_count(h: number, name: string): number",
            "Numero de listeners registrados para o evento.",
            __RTS_FN_NS_EVENTS_LISTENER_COUNT as *const u8,
        ))
        .member(func(
            "emit0",
            "__RTS_FN_NS_EVENTS_EMIT0",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::I64),
            "emit0(h: number, name: string): number",
            "Dispara `name` sem argumentos. Retorna 1 se havia listeners, 0 caso contrario.",
            __RTS_FN_NS_EVENTS_EMIT0 as *const u8,
        ))
        .member(func(
            "emit1",
            "__RTS_FN_NS_EVENTS_EMIT1",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr, AbiType::I64], AbiType::I64),
            "emit1(h: number, name: string, arg0: number): number",
            "Dispara `name` com 1 argumento i64. Sincrono sequencial Node-style — listeners chamados em ordem de registro na thread atual. Para dispatch paralelo fire-and-forget use `emit1_async`.",
            __RTS_FN_NS_EVENTS_EMIT1 as *const u8,
        ))
        .member(func(
            "emit0_async",
            "__RTS_FN_NS_EVENTS_EMIT0_ASYNC",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::I64),
            "emit0_async(h: number, name: string): number",
            "Dispara `name` sem args, fire-and-forget paralelo via tokio (cada listener vira `spawn_blocking`). ATENCAO: bench mostra que e' ~10× MAIS LENTO que `emit0` para listeners leves (atomic, memory ops, log). So' vale a pena quando cada listener faz trabalho pesado (>10µs cada): HTTP request, disk I/O, calculo numerico longo. Para tudo mais use `emit0` que e' sequencial mas drasticamente mais rapido.",
            __RTS_FN_NS_EVENTS_EMIT0_ASYNC as *const u8,
        ))
        .member(func(
            "emit1_async",
            "__RTS_FN_NS_EVENTS_EMIT1_ASYNC",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr, AbiType::I64], AbiType::I64),
            "emit1_async(h: number, name: string, arg0: number): number",
            "Variante async de `emit1`. Mesmo trade-off de `emit0_async`: usar so' quando listeners individuais sao pesados.",
            __RTS_FN_NS_EVENTS_EMIT1_ASYNC as *const u8,
        ))
        .done();
}
