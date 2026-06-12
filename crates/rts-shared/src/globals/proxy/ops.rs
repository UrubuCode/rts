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

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

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
    fn __RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY(
        obj: u64,
        key_ptr: *const u8,
        key_len: i64,
        descriptor: u64,
    ) -> u64;
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

// ── Shims extern-C p/ desacoplar `globals::function` (Fase 2.3) ───────────
// A classe primordial Function (migrada p/ `rts-primitives`) redireciona
// `.call/.apply` p/ traps de Proxy callable. Estes shims `#[no_mangle]` expõem
// `resolve_proxy`/`dispatch_apply` por símbolo (link no AOT; `add_fn!` no JIT).

/// `resolve_proxy` via ABI: escreve target/handler nos out-params e devolve 1
/// se `handle` for Proxy; 0 caso contrário (out-params intocados).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_PROXY_RESOLVE(
    handle: u64,
    out_target: *mut u64,
    out_handler: *mut u64,
) -> i64 {
    match resolve_proxy(handle) {
        Some((t, h)) => {
            unsafe {
                *out_target = t;
                *out_handler = h;
            }
            1
        }
        None => 0,
    }
}

/// `dispatch_apply` via símbolo (assinatura já ABI-compatível).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_PROXY_DISPATCH_APPLY(
    target: u64,
    handler: u64,
    this_arg: i64,
    args_handle: u64,
) -> i64 {
    dispatch_apply(target, handler, this_arg, args_handle)
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

/// (#98) `Object.keys(proxy)` — ECMA-262 OrdinaryOwnPropertyKeys filtra
/// para own properties **enumeraveis**. Para um Proxy, isso dispara o trap
/// `ownKeys` e, para cada chave, o trap `getOwnPropertyDescriptor` pra
/// checar `enumerable` (mesmo trace que Bun/Node produzem).
///
/// Diferente de `dispatch_own_keys` (usado por `Reflect.ownKeys`, que NAO
/// filtra), aqui filtramos. Quando nao ha trap correspondente, os
/// `forward_*` reproduzem o comportamento default do objeto.
pub fn dispatch_own_keys_enumerable(target: u64, handler: u64) -> u64 {
    let all = dispatch_own_keys(target, handler);
    // Extrai os handles de string do Vec retornado.
    let key_handles: Vec<u64> = with_entry(all, |e| match e {
        Some(Entry::Vec(v)) => v.iter().map(|x| *x as u64).collect(),
        _ => Vec::new(),
    });
    let mut kept: Vec<i64> = Vec::with_capacity(key_handles.len());
    for kh in key_handles {
        // [[GetOwnProperty]]: dispara trap getOwnPropertyDescriptor.
        let desc = dispatch_get_own_property_descriptor(target, handler, kh);
        if desc == 0 {
            continue;
        }
        // undefined => prop ausente, pula.
        let is_undef = with_entry(desc, |e| {
            matches!(e, Some(Entry::String(b)) if b.as_slice() == b"undefined")
        });
        if is_undef {
            continue;
        }
        // Le `enumerable` do descriptor Map. Ausente => trata como nao
        // enumeravel (spec: default false em descriptors retornados por
        // getOwnPropertyDescriptor; mas para objetos comuns o forward
        // sempre popula true/false).
        let enumerable = with_entry(desc, |e| match e {
            Some(Entry::Map(m)) => match m.get("enumerable") {
                Some(x) if *x == i64::MIN => false,
                Some(x) if *x == i64::MIN + 1 => true,
                Some(x) => *x != 0,
                None => false,
            },
            _ => false,
        });
        if enumerable {
            kept.push(kh as i64);
        }
    }
    alloc_entry(Entry::Vec(Box::new(kept)))
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
    let _ = crate::globals::function::ops::__RTS_FN_GL_FUNCTION_APPLY_TYPED(
        target, inst as i64, args_handle,
    );
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
    // (#98) Delega ao MAP_DEFINE_PROPERTY completo (trata value, writable,
    // enumerable, configurable, getter/setter `__get_`/`__set_` e TypeError
    // de redefinicao non-configurable). Antes essa fn era uma v0 que so'
    // tratava value+writable+enumerable, regredindo accessor descriptors
    // (`{ get(){} }`) quando Object.defineProperty passou a rotear por aqui.
    let Some(key_bytes) = with_entry(key_handle, |e| match e {
        Some(Entry::String(b)) => Some(b.clone()),
        _ => None,
    }) else {
        return 0;
    };
    unsafe {
        __RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY(
            target,
            key_bytes.as_ptr(),
            key_bytes.len() as i64,
            descriptor,
        );
    }
    1
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
    // (#795) JS spec: undefined quando prop nao existe. Handle string
    // "undefined" — TPL_COERCE_AUTO renderiza como "undefined".
    let Some(v) = value else { return alloc_entry(Entry::String(b"undefined".to_vec())) };
    // (cross-runtime #795) consulta flag tracking pra preservar
    // writable/enumerable definidos via defineProperty.
    let writable_bool = !crate::collections::map::is_non_writable(target, &key_str);
    let enumerable_bool = !crate::collections::map::is_non_enumerable(target, &key_str);
    // (#98/#1073/349) configurable rastreado via Object.defineProperty
    // (is_non_configurable). Antes hardcodava true aqui, regredindo
    // 349_object_descriptors quando getOwnPropertyDescriptor passou a
    // rotear pela versao _PROXY.
    let configurable_bool =
        !crate::collections::map::is_non_configurable(target, &key_str);
    let mut desc: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    desc.insert("value".to_string(), v);
    // Bool sentinels (i64::MIN+1 = true, i64::MIN = false) pra TPL_COERCE_AUTO
    // formatar como "true"/"false" em vez de inteiros raw.
    desc.insert("writable".to_string(), if writable_bool { i64::MIN + 1 } else { i64::MIN });
    desc.insert("enumerable".to_string(), if enumerable_bool { i64::MIN + 1 } else { i64::MIN });
    desc.insert("configurable".to_string(), if configurable_bool { i64::MIN + 1 } else { i64::MIN });
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
    use rts_engine::heap::handles::{Entry, alloc_entry};
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
