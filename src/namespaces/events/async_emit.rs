//! `events.emit_async` — dispatch fire-and-forget paralelo via tokio.
//!
//! Diferente de `emit0/emit1` (sincrono sequencial Node-style), aqui
//! cada listener vira uma task `tokio::spawn_blocking` no runtime
//! compartilhado (issue #399). Retorna imediatamente, ordem
//! indeterminada.
//!
//! Mantido em arquivo separado de `ops.rs` porque `ops.rs` faz parte
//! do `runtime_support` (compilado standalone via `build.rs`, sem
//! acesso ao `crate::runtime::async_rt`). Este arquivo so' compila no
//! crate principal — JIT registra o simbolo direto.

use crate::namespaces::gc::handles::{Entry, RtsEventsEmitter, with_entry};

fn snapshot(handle: u64, name: &str) -> Vec<u64> {
    with_entry(handle, |entry| match entry {
        Some(Entry::RtsEventsEmitter(e)) => e
            .listeners
            .get(name)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    })
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMIT0_ASYNC(
    handle: u64,
    name_ptr: *const u8,
    name_len: i64,
) -> i64 {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return 0;
    };
    let listeners = snapshot(handle, name);
    if listeners.is_empty() {
        return 0;
    }
    let rt = crate::runtime::async_rt::handle();
    for fp in listeners {
        rt.spawn_blocking(move || {
            // SAFETY: caller contract — fp e' func_addr de
            // `extern "C" fn()` registrada via `events.on(...)`.
            let f: extern "C" fn() = unsafe { std::mem::transmute(fp as usize) };
            f();
        });
    }
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMIT1_ASYNC(
    handle: u64,
    name_ptr: *const u8,
    name_len: i64,
    arg0: i64,
) -> i64 {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return 0;
    };
    let listeners = snapshot(handle, name);
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

// Mantemos o import explicito so' pra silenciar dead_code quando
// outros call paths nao usam — RtsEventsEmitter e' referenciado
// indiretamente via with_entry pattern match.
#[allow(dead_code)]
fn _keep_emitter_in_scope(_e: &RtsEventsEmitter) {}
