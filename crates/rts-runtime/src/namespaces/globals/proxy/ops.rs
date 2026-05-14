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
    fn __RTS_FN_NS_COLLECTIONS_MAP_KEYS(handle: u64) -> u64;
    fn __RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO(handle: u64) -> u64;
    fn __RTS_FN_GL_FUNCTION_APPLY(fn_h: u64, this_arg: i64, args_handle: u64) -> i64;
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

/// Trap `ownKeys(target)`. Retorna handle Vec<i64> com handles de string
/// das chaves. Sem trap, forward pra MAP_KEYS do target.
pub fn dispatch_own_keys(target: u64, handler: u64) -> u64 {
    let trap = lookup_trap(handler, "ownKeys");
    if trap == 0 {
        return unsafe { __RTS_FN_NS_COLLECTIONS_MAP_KEYS(target) };
    }
    let args = build_args_vec(&[target as i64]);
    let r = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, args) };
    // Trap deve retornar um Vec handle. Se nao for, devolve Vec vazio.
    let h = r as u64;
    let is_vec = with_entry(h, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec { h } else { alloc_entry(Entry::Vec(Box::new(Vec::new()))) }
}

/// Trap `getPrototypeOf(target)`. Retorna o handle do proto.
pub fn dispatch_get_proto(target: u64, handler: u64) -> u64 {
    let trap = lookup_trap(handler, "getPrototypeOf");
    if trap == 0 {
        return unsafe { __RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO(target) };
    }
    let args = build_args_vec(&[target as i64]);
    let r = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, args) };
    r as u64
}

/// Trap `apply(target, thisArg, argsArray)`. Quando target eh callable
/// e o user faz `proxy(args)`, esse caminho redireciona pra trap.
/// Sem trap, faz forward pra `Function.apply` no target.
pub fn dispatch_apply(target: u64, handler: u64, this_arg: i64, args_handle: u64) -> i64 {
    let trap = lookup_trap(handler, "apply");
    if trap == 0 {
        // Forward: invoca o target como Function.
        return unsafe { __RTS_FN_GL_FUNCTION_APPLY(target, this_arg, args_handle) };
    }
    // Trap recebe (target, thisArg, argsArray) — empacota como Vec de 3 itens.
    let trap_args = build_args_vec(&[target as i64, this_arg, args_handle as i64]);
    unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, trap_args) }
}

/// Trap `construct(target, args, newTarget)`. v0 ignora newTarget.
/// Sem trap, faz forward: aloca instancia (Map vazio) + apply target
/// como construtor (mesma logica de Reflect.construct).
pub fn dispatch_construct(target: u64, handler: u64, args_handle: u64) -> u64 {
    let trap = lookup_trap(handler, "construct");
    if trap == 0 {
        // Forward: cria Map vazio + apply target com this=instancia.
        let inst = alloc_entry(Entry::Map(Box::new(indexmap::IndexMap::new())));
        let _ = unsafe { __RTS_FN_GL_FUNCTION_APPLY(target, inst as i64, args_handle) };
        return inst;
    }
    let trap_args = build_args_vec(&[target as i64, args_handle as i64, target as i64]);
    let r = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, trap_args) };
    r as u64
}

/// Wrapper exposto pra codegen: `Reflect.construct(target, args)`. Quando
/// target eh Proxy, dispara trap construct. Senao, faz o caminho default
/// (alocar Map + apply). Mantido como fn separada do codegen pra evitar
/// duplicar logica em cada call site.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REFLECT_CONSTRUCT(target: u64, args_handle: u64) -> u64 {
    if let Some((real_target, handler)) = resolve_proxy(target) {
        return dispatch_construct(real_target, handler, args_handle);
    }
    // Forward default: aloca instancia + apply.
    let inst = alloc_entry(Entry::Map(Box::new(indexmap::IndexMap::new())));
    let _ = unsafe { __RTS_FN_GL_FUNCTION_APPLY(target, inst as i64, args_handle) };
    inst
}

/// Trap `setPrototypeOf(target, proto)`. Retorna 1 (true) na convencao
/// JS spec (failure-modes em invariants ficam pra phase com
/// preventExtensions real).
pub fn dispatch_set_proto(target: u64, handler: u64, proto: u64) -> i64 {
    let trap = lookup_trap(handler, "setPrototypeOf");
    if trap == 0 {
        // Forward: escreve __proto__ direto no target Map.
        let key = "__proto__";
        unsafe {
            __RTS_FN_NS_COLLECTIONS_MAP_SET(
                target,
                key.as_ptr(),
                key.len() as i64,
                proto as i64,
            );
        }
        return 1;
    }
    let trap_args = build_args_vec(&[target as i64, proto as i64]);
    let r = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, trap_args) };
    if r != 0 { 1 } else { 0 }
}

/// Wrapper: `Reflect.setPrototypeOf(target, proto)`. Detecta Proxy e
/// despacha a trap correspondente; senao faz forward direto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REFLECT_SET_PROTOTYPE_OF(target: u64, proto: u64) -> i64 {
    if let Some((real_target, handler)) = resolve_proxy(target) {
        return dispatch_set_proto(real_target, handler, proto);
    }
    let key = "__proto__";
    unsafe {
        __RTS_FN_NS_COLLECTIONS_MAP_SET(
            target,
            key.as_ptr(),
            key.len() as i64,
            proto as i64,
        );
    }
    1
}

/// Trap `defineProperty(target, key, descriptor)`. Retorna 1 (true) em
/// sucesso, 0 em falha (trap retornou falsy). Sem trap, extrai
/// `descriptor.value` e faz `target[key] = value` (mesma semantica de
/// `Reflect.defineProperty` v0 sem proxy).
pub fn dispatch_define_property(
    target: u64,
    handler: u64,
    key_handle: u64,
    descriptor: u64,
) -> i64 {
    let trap = lookup_trap(handler, "defineProperty");
    if trap == 0 {
        return forward_define_property(target, key_handle, descriptor);
    }
    let trap_args = build_args_vec(&[target as i64, key_handle as i64, descriptor as i64]);
    let r = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, trap_args) };
    if r != 0 { 1 } else { 0 }
}

fn forward_define_property(target: u64, key_handle: u64, descriptor: u64) -> i64 {
    let Some(key_bytes) = with_entry(key_handle, |e| match e {
        Some(Entry::String(b)) => Some(b.clone()),
        _ => None,
    }) else {
        return 0;
    };
    let value: i64 = with_entry(descriptor, |e| match e {
        Some(Entry::Map(m)) => m.get("value").copied().unwrap_or(0),
        _ => 0,
    });
    use crate::namespaces::gc::handles::with_entry_mut;
    let key_str = String::from_utf8_lossy(&key_bytes).into_owned();
    let ok = with_entry_mut(target, |e| match e {
        Some(Entry::Map(m)) => {
            m.insert(key_str, value);
            true
        }
        _ => false,
    });
    if ok { 1 } else { 0 }
}

/// Trap `getOwnPropertyDescriptor(target, key)`. Retorna handle Map com
/// descriptor sintetizado, ou 0 quando ausente. Sem trap, monta o
/// descriptor v0 (writable/enumerable/configurable=true) a partir do
/// slot do target Map.
pub fn dispatch_get_own_property_descriptor(
    target: u64,
    handler: u64,
    key_handle: u64,
) -> u64 {
    let trap = lookup_trap(handler, "getOwnPropertyDescriptor");
    if trap == 0 {
        return forward_get_own_property_descriptor(target, key_handle);
    }
    let trap_args = build_args_vec(&[target as i64, key_handle as i64]);
    let r = unsafe { __RTS_FN_RT_INVOKE_AUTO(trap, 0, trap_args) };
    r as u64
}

fn forward_get_own_property_descriptor(target: u64, key_handle: u64) -> u64 {
    let Some(key_bytes) = with_entry(key_handle, |e| match e {
        Some(Entry::String(b)) => Some(b.clone()),
        _ => None,
    }) else {
        return 0;
    };
    let key_str = String::from_utf8_lossy(&key_bytes).into_owned();
    let value: Option<i64> = with_entry(target, |e| match e {
        Some(Entry::Map(m)) => m.get(&key_str).copied(),
        _ => None,
    });
    let Some(v) = value else {
        // (cross-runtime #795) JS spec: getOwnPropertyDescriptor de key inexistente
        // retorna undefined, nao null. Aloca string handle "undefined" — codegen
        // marca callsite como Handle (ValTy::Handle), TPL_COERCE_AUTO renderiza.
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    };
    let mut desc: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    desc.insert("value".to_string(), v);
    desc.insert("writable".to_string(), 1);
    desc.insert("enumerable".to_string(), 1);
    desc.insert("configurable".to_string(), 1);
    alloc_entry(Entry::Map(Box::new(desc)))
}

/// Wrappers expostos pra codegen — trocam os entry-points existentes
/// em globals/reflect/ops.rs por versoes proxy-aware.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REFLECT_DEFINE_PROPERTY_PROXY(
    target: u64,
    key_handle: u64,
    descriptor: u64,
) -> i64 {
    if let Some((real_target, handler)) = resolve_proxy(target) {
        return dispatch_define_property(real_target, handler, key_handle, descriptor);
    }
    forward_define_property(target, key_handle, descriptor)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_PROXY(
    target: u64,
    key_handle: u64,
) -> u64 {
    if let Some((real_target, handler)) = resolve_proxy(target) {
        return dispatch_get_own_property_descriptor(real_target, handler, key_handle);
    }
    forward_get_own_property_descriptor(target, key_handle)
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
