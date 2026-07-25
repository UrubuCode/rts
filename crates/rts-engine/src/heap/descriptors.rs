//! Tracking global de flags de property-descriptor (writable/enumerable/
//! configurable) por `(handle, key)`. Movido de `rts-shared::collections::map`
//! (LAYERING FIX 2026-07-24) — esta é semântica engine-level de
//! `[[DefineOwnProperty]]`/`[[Get]]`/`[[Enumerate]]` (ECMA-262 §6.2.6), não
//! algo específico do backing store `IndexMap<String,i64>` de rts-shared. Vive
//! no motor para que `reflect`/`proxy` (primordiais, `rts-primitives`) e o
//! Map/Set (`rts-shared`) consultem o MESMO estado sem uma dependência
//! rts-primitives → rts-shared (que violaria a direção do grafo de crates).
//!
//! Handles cruzam threads (o `HandleTable` é shard global), então o tracking
//! usa `Mutex<HashSet<...>>` globais (não thread-local) — a mesma lição do
//! frozen/sealed tracking (#479 follow-up).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Set global de `(handle, key)` marcados como não-enumeráveis via
/// `Object.defineProperty(obj, key, { enumerable: false, ... })`. Usado por
/// Object.keys/values/entries e for-in para filtrar.
fn non_enumerable_set() -> &'static Mutex<HashSet<(u64, String)>> {
    static S: OnceLock<Mutex<HashSet<(u64, String)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn is_non_enumerable(handle: u64, key: &str) -> bool {
    non_enumerable_set()
        .lock()
        .unwrap()
        .contains(&(handle, key.to_string()))
}

pub fn mark_non_enumerable(handle: u64, key: &str) {
    non_enumerable_set()
        .lock()
        .unwrap()
        .insert((handle, key.to_string()));
}

pub fn unmark_non_enumerable(handle: u64, key: &str) {
    non_enumerable_set()
        .lock()
        .unwrap()
        .remove(&(handle, key.to_string()));
}

/// Tracking de `writable=false` definido via `Object.defineProperty` /
/// `Reflect.defineProperty`. Usado por `Reflect.getOwnPropertyDescriptor` para
/// retornar `writable` correto.
fn non_writable_set() -> &'static Mutex<HashSet<(u64, String)>> {
    static S: OnceLock<Mutex<HashSet<(u64, String)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn is_non_writable(handle: u64, key: &str) -> bool {
    non_writable_set()
        .lock()
        .unwrap()
        .contains(&(handle, key.to_string()))
}

pub fn mark_non_writable(handle: u64, key: &str) {
    non_writable_set()
        .lock()
        .unwrap()
        .insert((handle, key.to_string()));
}

pub fn unmark_non_writable(handle: u64, key: &str) {
    non_writable_set()
        .lock()
        .unwrap()
        .remove(&(handle, key.to_string()));
}

/// Tracking de `configurable=false`. Default JS para `Object.defineProperty`
/// (sem flag) é `false`; mas RTS só marca quando explicitamente `false`, para
/// compat com object literal (campo de literal é `configurable=true`).
fn non_configurable_set() -> &'static Mutex<HashSet<(u64, String)>> {
    static S: OnceLock<Mutex<HashSet<(u64, String)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn is_non_configurable(handle: u64, key: &str) -> bool {
    non_configurable_set()
        .lock()
        .unwrap()
        .contains(&(handle, key.to_string()))
}

pub fn mark_non_configurable(handle: u64, key: &str) {
    non_configurable_set()
        .lock()
        .unwrap()
        .insert((handle, key.to_string()));
}
