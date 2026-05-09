//! Proxy traps (#218 Phase 1).
//!
//! Invocacao: `MAP_GET_CHAIN`/`MAP_SET`/`MAP_HAS`/`MAP_DELETE` checam
//! `resolve_proxy(handle)` e, se for Proxy, delegam pra dispatch_*.
//!
//! Cada dispatch_* faz:
//!   1. Olha se handler[trap_name] existe e nao e' 0/null.
//!   2. Se existe: chama trap(target, key_handle [, value_handle]) via
//!      INVOKE_AUTO com Vec handle de args.
//!   3. Se nao existe: forward direto pro target (caminho default JS).

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

unsafe extern "C" {
    fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
    fn __RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN(
        handle: u64,
        key_ptr: *const u8,
        key_len: i64,
    ) -> i64;
    fn __RTS_FN_NS_COLLECTIONS_MAP_SET(
        handle: u64,
        key_ptr: *const u8,
        key_len: i64,
        value: i64,
    );
    fn __RTS_FN_NS_COLLECTIONS_MAP_HAS(
        handle: u64,
        key_ptr: *const u8,
        key_len: i64,
    ) -> i64;
    fn __RTS_FN_NS_COLLECTIONS_MAP_DELETE(
        handle: u64,
        key_ptr: *const u8,
        key_len: i64,
    ) -> i64;
}

/// Construtor Proxy: aloca Entry::Proxy { target, handler }.
/// Retorna 0 se o handler nao for Map (validacao minima — JS lanca
/// TypeError, mas v0 retorna 0 pra evitar throw desnecessario).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROXY_NEW(target: u64, handler: u64) -> u64 {
    // Validacao basica: handler precisa ser Map.
    let handler_ok = with_entry(handler, |e| matches!(e, Some(Entry::Map(_))));
    if !handler_ok {
        return 0;
    }
    alloc_entry(Entry::Proxy { target, handler })
}

/// Tenta resolver handle como Proxy. Devolve (target, handler) ou None.
pub fn resolve_proxy(handle: u64) -> Option<(u64, u64)> {
    with_entry(handle, |e| match e {
        Some(Entry::Proxy { target, handler }) => Some((*target, *handler)),
        _ => None,
    })
}

/// Olha o slot `trap_name` no handler Map. Retorna 0 se ausente ou nao
/// for handle valido — caller deve fazer fallback ao target.
fn lookup_trap(handler: u64, trap_name: &str) -> i64 {
    with_entry(handler, |e| match e {
        Some(Entry::Map(m)) => m.get(trap_name).copied().unwrap_or(0),
        _ => 0,
    })
}

/// Empacota args como Entry::Vec(slots).
fn build_args_vec(args: &[i64]) -> u64 {
    alloc_entry(Entry::Vec(Box::new(args.to_vec())))
}

/// Aloca um string handle (key) para passar como arg da trap.
fn alloc_key_handle(key: &str) -> u64 {
    alloc_entry(Entry::String(key.as_bytes().to_vec()))
}

/// Trap `get(target, prop, receiver)`. v0 ignora receiver.
pub fn dispatch_get(target: u64, handler: u64, key: &str) -> i64 {
    let trap = lookup_trap(handler, "get");
    if trap == 0 {
        // Forward pro target.
        let bytes = key.as_bytes();
        return unsafe {
            __RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN(target, bytes.as_ptr(), bytes.len() as i64)
        };
    }
    let key_h = alloc_key_handle(key);
    let args = build_args_vec(&[target as i64, key_h as i64]);
    unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, args) }
}

/// Trap `set(target, prop, value, receiver)`. v0 ignora receiver.
pub fn dispatch_set(target: u64, handler: u64, key: &str, value: i64) {
    let trap = lookup_trap(handler, "set");
    if trap == 0 {
        let bytes = key.as_bytes();
        unsafe {
            __RTS_FN_NS_COLLECTIONS_MAP_SET(target, bytes.as_ptr(), bytes.len() as i64, value);
        }
        return;
    }
    let key_h = alloc_key_handle(key);
    let args = build_args_vec(&[target as i64, key_h as i64, value]);
    let _ = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, args) };
}

/// Trap `has(target, prop)`. Retorna 0/1.
pub fn dispatch_has(target: u64, handler: u64, key: &str) -> i64 {
    let trap = lookup_trap(handler, "has");
    if trap == 0 {
        let bytes = key.as_bytes();
        return unsafe {
            __RTS_FN_NS_COLLECTIONS_MAP_HAS(target, bytes.as_ptr(), bytes.len() as i64)
        };
    }
    let key_h = alloc_key_handle(key);
    let args = build_args_vec(&[target as i64, key_h as i64]);
    let r = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, args) };
    if r != 0 { 1 } else { 0 }
}

/// Trap `deleteProperty(target, prop)`. Retorna 0/1.
pub fn dispatch_delete(target: u64, handler: u64, key: &str) -> i64 {
    let trap = lookup_trap(handler, "deleteProperty");
    if trap == 0 {
        let bytes = key.as_bytes();
        return unsafe {
            __RTS_FN_NS_COLLECTIONS_MAP_DELETE(target, bytes.as_ptr(), bytes.len() as i64)
        };
    }
    let key_h = alloc_key_handle(key);
    let args = build_args_vec(&[target as i64, key_h as i64]);
    let r = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, args) };
    if r != 0 { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::gc::handles::{Entry, alloc_entry};
    use indexmap::IndexMap;

    #[test]
    fn proxy_new_with_invalid_handler_returns_zero() {
        let target = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
        let bogus_handler = 0u64;
        let p = __RTS_FN_GL_PROXY_NEW(target, bogus_handler);
        assert_eq!(p, 0);
    }

    #[test]
    fn proxy_new_with_map_handler_succeeds() {
        let target = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
        let handler = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
        let p = __RTS_FN_GL_PROXY_NEW(target, handler);
        assert_ne!(p, 0);
        let resolved = resolve_proxy(p);
        assert_eq!(resolved, Some((target, handler)));
    }

    #[test]
    fn proxy_without_get_trap_forwards_to_target() {
        let mut tm: IndexMap<String, i64> = IndexMap::new();
        tm.insert("x".to_string(), 42);
        let target = alloc_entry(Entry::Map(Box::new(tm)));
        let handler = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
        let p = __RTS_FN_GL_PROXY_NEW(target, handler);
        let v = dispatch_get(target, handler, "x");
        assert_eq!(v, 42);
        let _ = p;
    }
}
