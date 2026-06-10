//! `events` namespace — EventEmitter primitivo handle-based.
//!
//! Listeners sao function pointers raw (`func_addr` em i64). Caller
//! (codegen) materializa o endereco da fn via `Expr::Ident` → `func_addr`
//! e passa pra `events.on`. `events.emit*` invoca o ponteiro via
//! `unsafe transmute` para a signature apropriada.
//!
//! `node:events` (#290) é wrapper TS sobre este namespace. Migrado ao modelo
//! `#[rts_namespace]` (stage 2c, `docs/specs/rts-core-engine.md`).

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;

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

/// Sistema de eventos primitivo. Listeners sao function pointers raw, invocados via transmute para extern "C" fn.
#[rts_namespace(events)]
impl EventsNs {
    /// Aloca um EventEmitter e retorna o handle.
    #[rts_fn]
    pub fn emitter_new() -> Handle {
        alloc_entry(Entry::RtsEventsEmitter(Box::new(
            RtsEventsEmitter::default(),
        )))
    }

    /// Libera o EventEmitter.
    #[rts_fn]
    pub fn emitter_free(h: Handle) {
        free_handle(h);
    }

    /// Registra um listener (function pointer raw) para o evento.
    #[rts_fn(ts = "on(h: number, name: string, fnPtr: number): void")]
    pub fn on(h: Handle, name: Str, fn_ptr: U64) {
        let key = name.to_string();
        with_emitter_mut(h, (), |e| {
            e.listeners.entry(key).or_default().push(fn_ptr);
        });
    }

    /// Remove o primeiro listener com o ponteiro especificado.
    #[rts_fn(ts = "off(h: number, name: string, fnPtr: number): void")]
    pub fn off(h: Handle, name: Str, fn_ptr: U64) {
        with_emitter_mut(h, (), |e| {
            if let Some(list) = e.listeners.get_mut(name) {
                if let Some(idx) = list.iter().position(|&p| p == fn_ptr) {
                    list.remove(idx);
                }
            }
        });
    }

    /// Remove todos os listeners do evento.
    #[rts_fn]
    pub fn remove_all_listeners(h: Handle, name: Str) {
        with_emitter_mut(h, (), |e| {
            e.listeners.remove(name);
        });
    }

    /// Numero de listeners registrados para o evento.
    #[rts_fn]
    pub fn listener_count(h: Handle, name: Str) -> I64 {
        with_emitter(h, 0, |e| {
            e.listeners.get(name).map(|l| l.len() as i64).unwrap_or(0)
        })
    }

    /// Dispara `name` sem argumentos. Retorna 1 se havia listeners, 0 caso contrario.
    #[rts_fn]
    pub fn emit0(h: Handle, name: Str) -> I64 {
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
    #[rts_fn]
    pub fn emit1(h: Handle, name: Str, arg0: I64) -> I64 {
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
    #[rts_fn]
    pub fn emit0_async(h: Handle, name: Str) -> I64 {
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
    #[rts_fn]
    pub fn emit1_async(h: Handle, name: Str, arg0: I64) -> I64 {
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
}
