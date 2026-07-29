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

use rts_engine::abi::ty::Handle;
use rts_engine::Engine;

use rts_engine::heap::handles::{
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
#[rtse::function(module = "events", value = "emitter_new", ret_ts = "number")]
fn emitter_new() -> Handle {
    alloc_entry(Entry::RtsEventsEmitter(Box::new(
        RtsEventsEmitter::default(),
    )))
}

/// Libera o EventEmitter.
#[rtse::function(module = "events", value = "emitter_free")]
fn emitter_free(h: Handle) {
    free_handle(h);
}

/// Registra um listener (function pointer raw) para o evento.
#[rtse::function(module = "events", value = "on")]
fn on(h: Handle, name: &str, fn_ptr: u64) {
    let key = name.to_string();
    with_emitter_mut(h, (), |e| {
        e.listeners.entry(key).or_default().push(fn_ptr);
    });
}

/// Remove o primeiro listener com o ponteiro especificado.
#[rtse::function(module = "events", value = "off")]
fn off(h: Handle, name: &str, fn_ptr: u64) {
    with_emitter_mut(h, (), |e| {
        if let Some(list) = e.listeners.get_mut(name) {
            if let Some(idx) = list.iter().position(|&p| p == fn_ptr) {
                list.remove(idx);
            }
        }
    });
}

/// Remove todos os listeners do evento.
#[rtse::function(module = "events", value = "remove_all_listeners")]
fn remove_all_listeners(h: Handle, name: &str) {
    with_emitter_mut(h, (), |e| {
        e.listeners.remove(name);
    });
}

/// Numero de listeners registrados para o evento.
#[rtse::function(module = "events", value = "listener_count")]
fn listener_count(h: Handle, name: &str) -> i64 {
    with_emitter(h, 0, |e| {
        e.listeners.get(name).map(|l| l.len() as i64).unwrap_or(0)
    })
}

/// Dispara `name` sem argumentos. Retorna 1 se havia listeners, 0 caso contrario.
#[rtse::function(module = "events", value = "emit0")]
fn emit0(h: Handle, name: &str) -> i64 {
    // Snapshot a lista antes de chamar — caller pode `off` durante o
    // dispatch sem invalidar o iterador.
    let snapshot: Vec<u64> = with_emitter(h, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if snapshot.is_empty() {
        return 0;
    }
    for fp in &snapshot {
        if !callable_fp(*fp) {
            continue;
        }
        // SAFETY: caller contract — fp veio de `func_addr` de uma user fn
        // registrada com signature compativel `extern "C" fn()`.
        let f: extern "C" fn() = unsafe { std::mem::transmute(*fp as usize) };
        f();
    }
    1
}

/// Whether `fp` is a plausible RAW code address — not null, not a NaN-boxed
/// PolyValue word. The `events` namespace is a RAW-fp API (old-engine model: the
/// callback is a `func_addr`). The new engine reifies a function as a TAG_FUNCTION
/// PolyValue (top 13 bits = the negative-qNaN box), NOT a raw pointer; transmuting
/// such a word — or a `0` (a function arg the new engine could not lower to an fp) —
/// to a code address and calling it segfaults. Skip a non-callable fp instead.
fn callable_fp(fp: u64) -> bool {
    fp != 0 && (fp & 0xFFF8_0000_0000_0000) != 0xFFF8_0000_0000_0000
}

/// Dispara `name` com 1 argumento i64. Sincrono sequencial Node-style — listeners chamados em ordem de registro na thread atual. Para dispatch paralelo fire-and-forget use `emit1_async`.
#[rtse::function(module = "events", value = "emit1")]
fn emit1(h: Handle, name: &str, arg0: i64) -> i64 {
    let snapshot: Vec<u64> = with_emitter(h, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if snapshot.is_empty() {
        return 0;
    }
    for fp in &snapshot {
        if !callable_fp(*fp) {
            continue;
        }
        // SAFETY: caller contract — fp aceita `extern "C" fn(i64)`.
        let f: extern "C" fn(i64) = unsafe { std::mem::transmute(*fp as usize) };
        f(arg0);
    }
    1
}

/// Dispara `name` sem args, fire-and-forget paralelo via tokio (cada listener vira `spawn_blocking`). ATENCAO: bench mostra que e' ~10× MAIS LENTO que `emit0` para listeners leves (atomic, memory ops, log). So' vale a pena quando cada listener faz trabalho pesado (>10µs cada): HTTP request, disk I/O, calculo numerico longo. Para tudo mais use `emit0` que e' sequencial mas drasticamente mais rapido.
#[rtse::function(module = "events", value = "emit0_async")]
fn emit0_async(h: Handle, name: &str) -> i64 {
    let listeners: Vec<u64> = with_emitter(h, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if listeners.is_empty() {
        return 0;
    }
    let rt = crate::runtime::async_rt::handle();
    for fp in listeners {
        if !callable_fp(fp) {
            continue;
        }
        rt.spawn_blocking(move || {
            // SAFETY: caller contract — `extern "C" fn()` via `events.on`.
            let f: extern "C" fn() = unsafe { std::mem::transmute(fp as usize) };
            f();
        });
    }
    1
}

/// Variante async de `emit1`. Mesmo trade-off de `emit0_async`: usar so' quando listeners individuais sao pesados.
#[rtse::function(module = "events", value = "emit1_async")]
fn emit1_async(h: Handle, name: &str, arg0: i64) -> i64 {
    let listeners: Vec<u64> = with_emitter(h, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if listeners.is_empty() {
        return 0;
    }
    let rt = crate::runtime::async_rt::handle();
    for fp in listeners {
        if !callable_fp(fp) {
            continue;
        }
        rt.spawn_blocking(move || {
            // SAFETY: caller contract — `extern "C" fn(i64)`.
            let f: extern "C" fn(i64) = unsafe { std::mem::transmute(fp as usize) };
            f(arg0);
        });
    }
    1
}

/// Registra a namespace `events` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.module("events", |m| {
        m.doc("Sistema de eventos primitivo. Listeners sao function pointers raw, invocados via transmute para extern \"C\" fn.");
        m.registry(emitter_new_entry());
        m.registry(emitter_free_entry());
        m.registry(on_entry());
        m.registry(off_entry());
        m.registry(remove_all_listeners_entry());
        m.registry(listener_count_entry());
        m.registry(emit0_entry());
        m.registry(emit1_entry());
        m.registry(emit0_async_entry());
        m.registry(emit1_async_entry());
    });
}
