//! Implementacao dos membros de `rts:events`. Listeners sao function
//! pointers raw (`u64` materializado de `func_addr`). Invocacao via
//! `unsafe transmute` para `extern "C" fn(...)` apropriada — apenas
//! `emit0` e `emit1` (ate' 1 arg i64) sao suportados nesta fase. Args
//! adicionais (`emit2`, `emit_arr`) ficam follow-up.

use crate::namespaces::gc::handles::{
    Entry, RtsEventsEmitter, alloc_entry, free_handle, shard_for_handle,
};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn with_emitter<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&RtsEventsEmitter) -> R,
{
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::RtsEventsEmitter(e)) => f(e.as_ref()),
        _ => default,
    }
}

fn with_emitter_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut RtsEventsEmitter) -> R,
{
    let mut guard = shard_for_handle(handle).lock().unwrap();
    match guard.get_mut(handle) {
        Some(Entry::RtsEventsEmitter(e)) => f(e.as_mut()),
        _ => default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMITTER_NEW() -> u64 {
    alloc_entry(Entry::RtsEventsEmitter(Box::new(RtsEventsEmitter::default())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMITTER_FREE(handle: u64) {
    free_handle(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_ON(
    handle: u64,
    name_ptr: *const u8,
    name_len: i64,
    fn_ptr: u64,
) {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return;
    };
    let key = name.to_string();
    with_emitter_mut(handle, (), |e| {
        e.listeners.entry(key).or_default().push(fn_ptr);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_OFF(
    handle: u64,
    name_ptr: *const u8,
    name_len: i64,
    fn_ptr: u64,
) {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return;
    };
    with_emitter_mut(handle, (), |e| {
        if let Some(list) = e.listeners.get_mut(name) {
            if let Some(idx) = list.iter().position(|&p| p == fn_ptr) {
                list.remove(idx);
            }
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_REMOVE_ALL(
    handle: u64,
    name_ptr: *const u8,
    name_len: i64,
) {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return;
    };
    with_emitter_mut(handle, (), |e| {
        e.listeners.remove(name);
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_LISTENER_COUNT(
    handle: u64,
    name_ptr: *const u8,
    name_len: i64,
) -> i64 {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return 0;
    };
    with_emitter(handle, 0, |e| {
        e.listeners.get(name).map(|l| l.len() as i64).unwrap_or(0)
    })
}

/// Dispara `name` sem argumentos. Snapshot a lista de listeners antes
/// de chamar — caller pode `off` durante o dispatch sem invalidar o
/// iterador. Retorna 1 se havia ao menos 1 listener, 0 caso contrario.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMIT0(
    handle: u64,
    name_ptr: *const u8,
    name_len: i64,
) -> i64 {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return 0;
    };
    let snapshot: Vec<u64> = with_emitter(handle, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if snapshot.is_empty() {
        return 0;
    }
    for fp in &snapshot {
        // SAFETY: caller contract — fn_ptr veio de `func_addr` de uma
        // user fn registrada com signature compativel `extern "C" fn()`.
        let f: extern "C" fn() = unsafe { std::mem::transmute(*fp as usize) };
        f();
    }
    1
}

/// Dispara `name` com 1 argumento i64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EVENTS_EMIT1(
    handle: u64,
    name_ptr: *const u8,
    name_len: i64,
    arg0: i64,
) -> i64 {
    let Some(name) = str_from_abi(name_ptr, name_len) else {
        return 0;
    };
    let snapshot: Vec<u64> = with_emitter(handle, Vec::new(), |e| {
        e.listeners.get(name).cloned().unwrap_or_default()
    });
    if snapshot.is_empty() {
        return 0;
    }
    for fp in &snapshot {
        // SAFETY: caller contract — fn_ptr aceita `extern "C" fn(i64)`.
        let f: extern "C" fn(i64) = unsafe { std::mem::transmute(*fp as usize) };
        f(arg0);
    }
    1
}
