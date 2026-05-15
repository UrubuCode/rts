//! IndexMap<String, i64> — mapa de chave string para valor i64.
//!
//! Usa `indexmap::IndexMap` para preservar ordem de inserção, necessário
//! para implementar a ordem de enumeração de propriedades do JS:
//! - integer-indexed keys (`"0"`, `"1"`, `"2"`, ...) em ordem numérica
//!   ascendente;
//! - demais string keys em ordem de inserção.

use indexmap::IndexMap;

use super::super::gc::handles::{Entry, alloc_entry, free_handle, with_entry, with_entry_mut};

/// Reconhece "array index" no sentido do ECMA-262: string que representa
/// um u32 canônico (sem leading zeros exceto "0"; máximo 2^32 - 2).
/// Retorna o valor numérico para ordenação. Strings como "01", "+1", "1.0",
/// " 1" não são consideradas índices.
fn parse_array_index(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = s.parse().ok()?;
    if n == u32::MAX {
        return None;
    }
    Some(n)
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn with_map<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&IndexMap<String, i64>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_ref()),
        _ => default,
    })
}

fn with_map_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut IndexMap<String, i64>) -> R,
{
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_mut()),
        _ => default,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_NEW() -> u64 {
    alloc_entry(Entry::Map(Box::new(IndexMap::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_FREE(handle: u64) {
    free_handle(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_LEN(handle: u64) -> i64 {
    with_map(handle, -1, |m| m.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_HAS(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    // (#218) Proxy: trap `has(target, prop)` ou forward.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_has(target, handler, key);
    }
    with_map(handle, 0, |m| if m.contains_key(key) { 1 } else { 0 })
}

/// Retorna o valor associado a `key`, ou 0 se ausente.
/// (0 tambem e valor valido — use map_has para distinguir.)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GET(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    // (#218) Proxy: dispatch get trap quando handle eh Proxy.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_get(target, handler, key);
    }
    with_map(handle, 0, |m| m.get(key).copied().unwrap_or(0))
}

/// Variante "direto" do map_get: NAO faz dispatch de Proxy. Usado pelos
/// caminhos do codegen que precisam de lookup raw — ex: getter sentinel
/// `__get_<key>` que so' faz sentido em Map normal e crasharia em Proxy
/// se o trap retornasse um valor nao-zero (interpretado como fn handle).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GET_DIRECT(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    with_map(handle, 0, |m| m.get(key).copied().unwrap_or(0))
}

/// (#264 PR5) Cria novo Map vazio com `__proto__` = proto_handle.
/// Implementa `Object.create(proto)`. Quando `proto == 0`, equivale a
/// `Object.create(null)` — slot __proto__ setado explicit como 0 para
/// que MAP_GET_PROTO retorne null em vez do default Object.prototype.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_CREATE(proto: u64) -> u64 {
    let h = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    with_map_mut(h, (), |m| {
        m.insert("__proto__".to_string(), proto as i64);
    });
    h
}

/// (#162) `Object.create(proto, descriptors)` 2-arg variant — aplica
/// descriptors ao objeto. Cada entry de `descs` deve ser
/// `{ key: { value, writable?, enumerable?, configurable? } }`.
/// Para v0, extraimos so o `value` e fazemos MAP_SET — meta de
/// writable/enumerable/configurable nao eh enforced ainda.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_APPLY_DESCRIPTORS(target: u64, descs: u64) {
    let pairs: Vec<(String, i64)> = with_entry(descs, |e| match e {
        Some(Entry::Map(m)) => m
            .iter()
            .filter(|(k, _)| !k.starts_with("__"))
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    for (key, desc_h_i) in pairs {
        let desc_h = desc_h_i as u64;
        let value: i64 = with_entry(desc_h, |e| match e {
            Some(Entry::Map(d)) => d.get("value").copied().unwrap_or(0),
            _ => desc_h_i,
        });
        with_map_mut(target, (), |m| {
            m.insert(key.clone(), value);
        });
    }
}

/// (#264 PR5) Verifica se `key` existe nas own props de `handle`
/// (sem seguir __proto__). Implementa \`obj.hasOwnProperty(key)\`.
/// Retorna 1 se own, 0 caso contrario.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    // Arrays (Vec): "length" eh own; chaves de indice (parsam como
    // uint32) sao own quando < length. Demais nao.
    let vec_result: Option<i64> = with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => {
            if key == "length" {
                Some(1)
            } else if let Some(n) = parse_array_index(key) {
                Some(if (n as usize) < v.len() { 1 } else { 0 })
            } else {
                Some(0)
            }
        }
        _ => None,
    });
    if let Some(r) = vec_result {
        return r;
    }
    with_map(handle, 0, |m| if m.contains_key(key) { 1 } else { 0 })
}

/// (cross-runtime #753) Resolve um handle de "chave qualquer" para a
/// representacao canonica em string usada no IndexMap.
///
/// - `Entry::String(bytes)` -> string UTF-8 direta.
/// - `Entry::Symbol`        -> "@@sym:<handle>" (unico por symbol, sticky).
/// - outros types           -> None (caller decide).
fn key_handle_to_string(key_h: u64) -> Option<String> {
    with_entry(key_h, |e| match e {
        Some(Entry::String(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Some(Entry::Symbol { .. }) => Some(format!("@@sym:{key_h}")),
        _ => None,
    })
}

/// (cross-runtime #753) `key in obj` generico — dispatch baseado em
/// Entry type do obj. Aceita key como handle (String, Symbol, etc).
/// Retorna 1/0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_OBJ_HAS(obj_h: u64, key_h: u64) -> i64 {
    let key = match key_handle_to_string(key_h) {
        Some(s) => s,
        None => return 0,
    };
    // Vec: index `i` esta "in" array se 0 <= i < length E slot != hole.
    let vec_result: Option<i64> = with_entry(obj_h, |e| match e {
        Some(Entry::Vec(v)) => {
            if let Some(n) = parse_array_index(&key) {
                let slot = v.get(n as usize).copied();
                Some(match slot {
                    Some(s) if s != i64::MIN + 4 => 1,
                    _ => 0,
                })
            } else if key == "length" {
                Some(1)
            } else {
                Some(0)
            }
        }
        _ => None,
    });
    if let Some(r) = vec_result {
        return r;
    }
    with_map(obj_h, 0, |m| if m.contains_key(&key) { 1 } else { 0 })
}

/// (cross-runtime #753) `MAP_SET` aceitando key como handle (resolve
/// Symbol via repr canonica `@@sym:<handle>`). Para String key normal,
/// equivalente a MAP_SET.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_SET_KH(obj_h: u64, key_h: u64, value: i64) {
    let Some(key) = key_handle_to_string(key_h) else {
        return;
    };
    with_map_mut(obj_h, (), |m| {
        m.insert(key, value);
    });
}

/// (cross-runtime #753) `MAP_GET` aceitando key como handle. Retorna
/// 0 se ausente (use OBJ_HAS pra distinguir).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GET_KH(obj_h: u64, key_h: u64) -> i64 {
    let Some(key) = key_handle_to_string(key_h) else {
        return 0;
    };
    with_map(obj_h, 0, |m| m.get(&key).copied().unwrap_or(0))
}

/// (cross-runtime #753) Dispatcher universal pra `obj[key] = value`:
/// Vec   -> parse key como uint32 e VEC_SET (extending com holes se preciso).
/// Map   -> MAP_SET_KH (resolve Symbol via repr canonica).
/// outros-> noop.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_OBJ_SET(obj_h: u64, key_h: u64, value: i64) {
    // Vec path: tenta parse de array index a partir da key.
    let is_vec = with_entry(obj_h, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        if let Some(key) = key_handle_to_string(key_h) {
            if let Some(n) = parse_array_index(&key) {
                with_entry_mut(obj_h, |e| {
                    if let Some(Entry::Vec(v)) = e {
                        let idx = n as usize;
                        if idx >= v.len() {
                            v.resize(idx + 1, i64::MIN + 4);
                        }
                        v[idx] = value;
                    }
                });
            }
        }
        return;
    }
    // Map path (ou other -> noop).
    if let Some(key) = key_handle_to_string(key_h) {
        with_map_mut(obj_h, (), |m| {
            m.insert(key, value);
        });
    }
}

/// (cross-runtime #753) Dispatcher universal pra `obj[key]`:
/// Vec   -> parse key como uint32 e VEC_GET. "length" retorna len.
/// Map   -> MAP_GET_KH.
/// outros-> 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_OBJ_GET(obj_h: u64, key_h: u64) -> i64 {
    let Some(key) = key_handle_to_string(key_h) else {
        return 0;
    };
    let vec_result: Option<i64> = with_entry(obj_h, |e| match e {
        Some(Entry::Vec(v)) => {
            if key == "length" {
                Some(v.len() as i64)
            } else if let Some(n) = parse_array_index(&key) {
                Some(v.get(n as usize).copied().unwrap_or(i64::MIN + 2))
            } else {
                Some(0)
            }
        }
        _ => None,
    });
    if let Some(r) = vec_result {
        return r;
    }
    with_map(obj_h, 0, |m| m.get(&key).copied().unwrap_or(0))
}

/// (cross-runtime #793) `map.forEach((value, key, map) => ...)`.
/// JS spec: callback recebe (value, key, map). RTS aceita callback ate 3 args;
/// se aridade for menor, args extras sao ignorados (transmute pra arity certa).
/// Para Map, key e' string handle (alocado por iteracao).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_FOR_EACH(handle: u64, fn_ptr: u64) {
    if fn_ptr == 0 { return; }
    // Snapshot pares (key_string, value) — evita lock entre alocacoes.
    let pairs: Vec<(String, i64)> = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m
            .iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    // Resolve handle Function -> fn_ptr (ou usa ptr direto).
    let resolved = with_entry(fn_ptr, |entry| {
        if let Some(Entry::Function(fd)) = entry {
            Some((fd.fn_ptr, fd.bound_args.clone()))
        } else {
            None
        }
    });
    let (actual_ptr, bound): (u64, Vec<i64>) = resolved.unwrap_or((fn_ptr, Vec::new()));
    let n_bound = bound.len();
    // (cross-runtime #793) Para Set, callback recebe (value, value, set) — key
    // duplica como value2 (JS spec). Set storage no RTS usa Map<keyStr, 1>;
    // o "value" da iteracao e' a key (que e' o elemento adicionado).
    let is_set = handle_is_set_kind(handle);
    for (k, v) in pairs {
        let key_h = alloc_entry(Entry::String(k.into_bytes())) as i64;
        // Map: (value, key, map). Set: (value, value, set) — value e' o
        // key_h (elemento original), e o segundo arg e' o mesmo key_h.
        let (a, b) = if is_set { (key_h, key_h) } else { (v, key_h) };
        unsafe {
            use std::mem::transmute;
            match n_bound {
                0 => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(actual_ptr)(
                    a, b, handle as i64,
                ),
                1 => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(actual_ptr)(
                    bound[0], a, b, handle as i64,
                ),
                _ => 0,
            };
        }
    }
}

/// `set.forEach((value, value2, set) => ...)`. JS spec: para Set, value2 == value.
/// Reusa Map.forEach passando o value como key tambem.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_FOR_EACH(handle: u64, fn_ptr: u64) {
    if fn_ptr == 0 { return; }
    let pairs: Vec<String> = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => m
            .iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .map(|(k, _)| k.clone())
            .collect(),
        _ => Vec::new(),
    });
    let resolved = with_entry(fn_ptr, |entry| {
        if let Some(Entry::Function(fd)) = entry {
            Some(fd.fn_ptr)
        } else {
            None
        }
    });
    let actual_ptr = resolved.unwrap_or(fn_ptr);
    for k in pairs {
        let h1 = alloc_entry(Entry::String(k.clone().into_bytes())) as i64;
        let h2 = alloc_entry(Entry::String(k.into_bytes())) as i64;
        unsafe {
            use std::mem::transmute;
            transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(actual_ptr)(
                h1, h2, handle as i64,
            );
        }
    }
}

/// (cross-runtime #772) `obj.isPrototypeOf(other)` — walk `other.__proto__`
/// chain procurando `obj`. Retorna 1 se obj aparece na cadeia, 0 caso
/// contrario. Guard contra ciclos: profundidade maxima 64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_IS_PROTOTYPE_OF(obj: u64, other: u64) -> i64 {
    if obj == 0 || other == 0 { return 0; }
    let mut current = with_map(other, 0u64, |m| {
        m.get("__proto__").copied().unwrap_or(0) as u64
    });
    let mut depth = 0u32;
    while current != 0 && depth < 64 {
        if current == obj { return 1; }
        current = with_map(current, 0u64, |m| {
            m.get("__proto__").copied().unwrap_or(0) as u64
        });
        depth += 1;
    }
    0
}

/// (cross-runtime #788) `obj.propertyIsEnumerable(key)` — true se own
/// property + enumerable. Inherited (via __proto__) ja' retorna false
/// porque so' checa own. Para Vec: indices = enumerable, "length" = false.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_PROPERTY_IS_ENUMERABLE(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    let vec_result: Option<i64> = with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => {
            // "length" eh own mas non-enumerable em arrays.
            if key == "length" { Some(0) }
            else if let Some(n) = parse_array_index(key) {
                Some(if (n as usize) < v.len() { 1 } else { 0 })
            } else { Some(0) }
        }
        _ => None,
    });
    if let Some(r) = vec_result { return r; }
    // Map: own + enumerable. defineProperty com enumerable:false marca via
    // is_non_enumerable.
    let has_own = with_map(handle, false, |m| m.contains_key(key));
    if !has_own { return 0; }
    if is_non_enumerable(handle, key) { return 0; }
    1
}

/// (#264 PR4) Retorna valor de `key` seguindo cadeia `__proto__`.
/// Se a key nao existe no map, le `__proto__` (handle de outro Map) e
/// recursa. Retorna 0 quando atinge o fim da cadeia ou tipo invalido.
/// Guard contra ciclos: profundidade maxima 64.
///
/// Codifica o estado de busca em 2 valores: -1 = nao tem proto (parar),
/// 0 = nao tem key mas tem proto (continuar), >0 = tem key (retornar).
/// Na pratica nao distinguimos -1 e 0 porque ambos paramos em 0 retorno.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    // (#218) Proxy: se handle for Entry::Proxy, dispara trap `get` no handler
    // ou faz forward para target. Trap recebe (target, key_handle).
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_get(target, handler, key);
    }
    // (cross-runtime #745) Entry::ErrorObj — quando o handle e' Error e
    // a key e' name/message/stack, retorna string handle alocado.
    // Sem isso, `catch (e: any)` + `e.name` cai em map_get e retorna 0
    // (renderizado como "null").
    let err_field: Option<String> = with_entry(handle, |e| match e {
        Some(Entry::ErrorObj { message, name, .. }) => match key {
            "name" => Some(name.clone()),
            "message" => Some(message.clone()),
            // "stack" — fora de escopo (RTS nao captura stack trace string).
            _ => None,
        },
        _ => None,
    });
    if let Some(s) = err_field {
        return alloc_entry(Entry::String(s.into_bytes())) as i64;
    }
    let key_owned = key.to_string();
    let mut current = handle;
    let mut depth = 0u32;
    while current != 0 && depth < 64 {
        // 1. Tenta key no map current.
        let found = with_map(current, 0i64, |m| m.get(&key_owned).copied().unwrap_or(0));
        if found != 0 {
            return found;
        }
        // 2. Le __proto__ pra continuar walk.
        let next = with_map(current, 0i64, |m| m.get("__proto__").copied().unwrap_or(0));
        if next == 0 {
            return 0;
        }
        current = next as u64;
        depth += 1;
    }
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_SET(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
    value: i64,
) {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return;
    };
    // (#218) Proxy: trap `set(target, prop, value)` ou forward.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        crate::namespaces::globals::proxy::ops::dispatch_set(target, handler, key, value);
        return;
    }
    // (#479 follow-up) frozen impede mutacao; sealed so' impede add de novas keys.
    if is_map_frozen(handle) {
        return;
    }
    let key_owned = key.to_string();
    let sealed = is_map_sealed(handle);
    with_map_mut(handle, (), |m| {
        if sealed && !m.contains_key(&key_owned) {
            return;
        }
        m.insert(key_owned, value);
    });
}

/// Remove a chave. Retorna 1 se removida, 0 se ausente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_DELETE(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    // (#218) Proxy: trap `deleteProperty(target, prop)` ou forward.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_delete(target, handler, key);
    }
    // sealed/frozen impedem delete.
    if is_map_sealed(handle) {
        return 0;
    }
    with_map_mut(handle, 0, |m| if m.shift_remove(key).is_some() { 1 } else { 0 })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_CLEAR(handle: u64) {
    with_map_mut(handle, (), |m| m.clear());
}

/// `delete obj.prop` — recebe key como handle de string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_DELETE_AUTO(handle: u64, key_handle: u64) -> i64 {
    use super::super::gc::handles::{with_entry, Entry};
    let key_owned: Option<String> = with_entry(key_handle, |e| match e {
        Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        _ => None,
    });
    let Some(key) = key_owned else { return 1 };
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_delete(target, handler, &key);
    }
    if is_map_sealed(handle) {
        return 0;
    }
    with_map_mut(handle, 1, |m| { let _ = m.shift_remove(&key); 1 })
}

/// Shallow clone do map — aloca novo handle com mesmas (key, value) pairs.
/// Usado pelo desugar de `const { a, ...rest } = obj` (#312): rest e'
/// inicializado como clone, e em seguida o codegen emite map_delete para
/// cada key explicita.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_CLONE(handle: u64) -> u64 {
    let cloned: Option<IndexMap<String, i64>> =
        with_map(handle, None, |m| Some(m.clone()));
    match cloned {
        Some(m) => alloc_entry(Entry::Map(Box::new(m))),
        None => 0,
    }
}

/// Retorna a key na posição `idx` na ordem de enumeração definida pelo JS:
/// 1. integer-indexed keys (string que parseia para u32 sem leading zero,
///    exceto "0") em ordem numérica ascendente;
/// 2. demais string keys em ordem de inserção (preservada pelo IndexMap).
///
/// Usado por for-in. Retorna handle de string ou 0 se idx fora de range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_KEY_AT(handle: u64, idx: i64) -> u64 {
    if idx < 0 {
        return 0;
    }
    let key_opt: Option<String> = with_map(handle, None, |m| {
        let mut int_keys: Vec<(u32, &String)> = Vec::new();
        let mut str_keys: Vec<&String> = Vec::new();
        for k in m.keys() {
            match parse_array_index(k) {
                Some(n) => int_keys.push((n, k)),
                None => str_keys.push(k),
            }
        }
        int_keys.sort_by_key(|(n, _)| *n);
        let i = idx as usize;
        if i < int_keys.len() {
            Some(int_keys[i].1.clone())
        } else {
            str_keys.get(i - int_keys.len()).map(|s| (*s).clone())
        }
    });
    match key_opt {
        Some(s) => crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            s.as_ptr(),
            s.len() as i64,
        ),
        None => 0,
    }
}

/// (#266) Object.keys(obj) — retorna Vec<i64> com handles de strings dos
/// keys. Ordem: sorted asc (mesmo criterio de KEY_AT).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_KEYS(handle: u64) -> u64 {
    // (#218 phase2) Proxy: trap `ownKeys(target)` ou forward.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_own_keys(target, handler);
    }
    let keys: Vec<String> = with_map(handle, Vec::new(), |m| {
        // (#208) Filtra `__proto__` e nao-enumeraveis (defineProperty) —
        // JS spec: Object.keys retorna so own enumeravel.
        // Ordem JS (ECMA-262): integer-indexed keys ascendentes, depois
        // string keys em ordem de insercao.
        let mut int_keys: Vec<(u32, String)> = Vec::new();
        let mut str_keys: Vec<String> = Vec::new();
        for k in m.keys() {
            if k.as_str() == "__proto__" {
                continue;
            }
            // (#110) Slots internos de getter/setter — nao expor.
            if k.starts_with("__get_") || k.starts_with("__set_") {
                continue;
            }
            if is_non_enumerable(handle, k.as_str()) {
                continue;
            }
            match parse_array_index(k) {
                Some(n) => int_keys.push((n, k.clone())),
                None => str_keys.push(k.clone()),
            }
        }
        int_keys.sort_by_key(|(n, _)| *n);
        let mut out: Vec<String> = int_keys.into_iter().map(|(_, k)| k).collect();
        out.extend(str_keys);
        out
    });
    let mut vec: Vec<i64> = Vec::with_capacity(keys.len());
    for k in keys {
        let h = crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            k.as_ptr(),
            k.len() as i64,
        );
        vec.push(h as i64);
    }
    crate::namespaces::gc::handles::alloc_entry(
        crate::namespaces::gc::handles::Entry::Vec(Box::new(vec)),
    )
}

/// (#266) Object.values(obj) — retorna Vec<i64> com valores em ordem
/// JS: integer-indexed keys ascendentes, depois string keys em ordem
/// de insercao (mesmo criterio de Object.keys).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_VALUES(handle: u64) -> u64 {
    let vals: Vec<i64> = with_map(handle, Vec::new(), |m| {
        let mut int_entries: Vec<(u32, i64)> = Vec::new();
        let mut str_entries: Vec<i64> = Vec::new();
        for (k, v) in m.iter() {
            if k.as_str() == "__proto__" { continue; }
            if is_non_enumerable(handle, k.as_str()) { continue; }
            match parse_array_index(k) {
                Some(n) => int_entries.push((n, *v)),
                None => str_entries.push(*v),
            }
        }
        int_entries.sort_by_key(|(n, _)| *n);
        let mut out: Vec<i64> = int_entries.into_iter().map(|(_, v)| v).collect();
        out.extend(str_entries);
        out
    });
    crate::namespaces::gc::handles::alloc_entry(
        crate::namespaces::gc::handles::Entry::Vec(Box::new(vals)),
    )
}

/// (#208 / #479) `Object.entries(obj)` — retorna Vec de Vec [key_handle, value].
/// Cada par e' um Vec<i64> com 2 elementos: handle de string da key, e o
/// valor i64. Ordem: keys sorted asc (Object.entries spec).
///
/// Para Map JS (que preserva ordem de insercao), use
/// `__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES_INSERTION` que mantem
/// IndexMap order.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_ENTRIES(handle: u64) -> u64 {
    let pairs: Vec<(String, i64)> = with_map(handle, Vec::new(), |m| {
        // Ordem JS: integer-indexed keys ascendentes, depois string
        // keys em ordem de insercao.
        let mut int_entries: Vec<(u32, String, i64)> = Vec::new();
        let mut str_entries: Vec<(String, i64)> = Vec::new();
        for (k, v) in m.iter() {
            if k.as_str() == "__proto__" { continue; }
            if is_non_enumerable(handle, k.as_str()) { continue; }
            match parse_array_index(k) {
                Some(n) => int_entries.push((n, k.clone(), *v)),
                None => str_entries.push((k.clone(), *v)),
            }
        }
        int_entries.sort_by_key(|(n, _, _)| *n);
        let mut out: Vec<(String, i64)> = int_entries
            .into_iter()
            .map(|(_, k, v)| (k, v))
            .collect();
        out.extend(str_entries);
        out
    });
    let mut outer: Vec<i64> = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        let key_h = crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            k.as_ptr(),
            k.len() as i64,
        );
        let inner = crate::namespaces::gc::handles::alloc_entry(
            crate::namespaces::gc::handles::Entry::Vec(Box::new(vec![key_h as i64, v])),
        );
        outer.push(inner as i64);
    }
    crate::namespaces::gc::handles::alloc_entry(
        crate::namespaces::gc::handles::Entry::Vec(Box::new(outer)),
    )
}

/// `Map.prototype.entries()` / iterador de Map JS — preserva ordem de
/// insercao (IndexMap). Retorna Vec de Vec [key_handle, value].
///
/// Diferente de `MAP_ENTRIES` (que ordena por chave para Object.entries),
/// esta variante preserva a ordem original — necessario para Map iter
/// e `for (const [k, v] of m)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_ENTRIES_INSERTION(handle: u64) -> u64 {
    let pairs: Vec<(String, i64)> = with_map(handle, Vec::new(), |m| {
        m.iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    });
    let mut outer: Vec<i64> = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        let key_h = crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            k.as_ptr(),
            k.len() as i64,
        );
        let inner = crate::namespaces::gc::handles::alloc_entry(
            crate::namespaces::gc::handles::Entry::Vec(Box::new(vec![key_h as i64, v])),
        );
        outer.push(inner as i64);
    }
    crate::namespaces::gc::handles::alloc_entry(
        crate::namespaces::gc::handles::Entry::Vec(Box::new(outer)),
    )
}

/// (#771) Set thread-safe de handles marcados como non-extensible via
/// `Object.preventExtensions`. Permite `Object.isExtensible` retornar false
/// para handles previamente prevenidos. Sweep do GC nao limpa entradas —
/// no-op se o handle eh liberado (slot decode invalido ja' nao seria
/// extensivel mesmo).
fn prevented_set() -> &'static std::sync::Mutex<std::collections::HashSet<u64>> {
    static PREVENTED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<u64>>> =
        std::sync::OnceLock::new();
    PREVENTED.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// `Object.preventExtensions(obj)` — marca handle como non-extensible.
/// Retorna o proprio handle (JS spec).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_PREVENT_EXTENSIONS(h: u64) -> u64 {
    if h != 0 {
        prevented_set().lock().unwrap().insert(h);
    }
    h
}

/// `Object.isExtensible(obj)` — retorna 1 se nunca foi prevent-extension'd
/// e o handle eh valido, 0 caso contrario.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_IS_EXTENSIBLE(h: u64) -> i64 {
    if h == 0 {
        return 0;
    }
    if prevented_set().lock().unwrap().contains(&h) {
        return 0;
    }
    1
}

/// (#208 / #479) `Object.assign(target, source)` — copia own props de source
/// pra target. Retorna handle do target. Versao com multiplos sources e'
/// chamada repetidamente pelo codegen.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_ASSIGN(target: u64, source: u64) -> u64 {
    let pairs: Vec<(String, i64)> = with_map(source, Vec::new(), |m| {
        m.iter().map(|(k, v)| (k.clone(), *v)).collect()
    });
    with_map_mut(target, (), |m| {
        for (k, v) in pairs {
            m.insert(k, v);
        }
    });
    target
}

/// Object.getOwnPropertyNames — inclui keys non-enumerable (diferenca para keys).
/// Para Vec: ["0","1",...,"length"]. Para Map: todas as keys exceto __proto__.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_OBJECT_OWN_PROPERTY_NAMES(handle: u64) -> u64 {
    let result: Vec<i64> = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => {
            let mut int_keys: Vec<(u32, String)> = Vec::new();
            let mut str_keys: Vec<String> = Vec::new();
            for k in m.keys() {
                if k.as_str() == "__proto__" { continue; }
                // (#110) Slots internos `__get_*`/`__set_*` nao sao expostos.
                if k.starts_with("__get_") || k.starts_with("__set_") { continue; }
                match parse_array_index(k) {
                    Some(n) => int_keys.push((n, k.clone())),
                    None => str_keys.push(k.clone()),
                }
            }
            int_keys.sort_by_key(|(n, _)| *n);
            let mut out: Vec<i64> = Vec::with_capacity(int_keys.len() + str_keys.len());
            for (_, k) in int_keys {
                out.push(alloc_entry(Entry::String(k.into_bytes())) as i64);
            }
            for k in str_keys {
                out.push(alloc_entry(Entry::String(k.into_bytes())) as i64);
            }
            out
        }
        Some(Entry::Vec(v)) => {
            // JS spec: arrays incluem "length" como own property name nao-enumerable.
            let mut out: Vec<i64> = v.iter()
                .enumerate()
                .filter(|(_, x)| **x != i64::MIN + 4)
                .map(|(i, _)| alloc_entry(Entry::String(i.to_string().into_bytes())) as i64)
                .collect();
            out.push(alloc_entry(Entry::String(b"length".to_vec())) as i64);
            out
        }
        _ => Vec::new(),
    });
    alloc_entry(Entry::Vec(Box::new(result)))
}

/// Object.keys auto: se handle e' Map retorna keys; se Vec retorna ["0","1",...].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_OBJECT_KEYS_AUTO(handle: u64) -> u64 {
    // (#218 phase2) Proxy: trap `ownKeys(target)` ou forward.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_own_keys(target, handler);
    }
    let result: Vec<i64> = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => {
            // Ordem JS (ECMA-262 OrdinaryOwnPropertyKeys): integer-indexed
            // ascendentes, depois string keys em ordem de insercao.
            let mut int_keys: Vec<(u32, String)> = Vec::new();
            let mut str_keys: Vec<String> = Vec::new();
            for k in m.keys() {
                if k.as_str() == "__proto__" { continue; }
                // (#110) Slots internos `__get_<name>`/`__set_<name>` armazenam
                // accessor handles. Object.keys/getOwnPropertyNames JS spec nao
                // expoe estes — RTS os usa como mecanismo interno de getter/setter.
                if k.starts_with("__get_") || k.starts_with("__set_") { continue; }
                if is_non_enumerable(handle, k.as_str()) { continue; }
                match parse_array_index(k) {
                    Some(n) => int_keys.push((n, k.clone())),
                    None => str_keys.push(k.clone()),
                }
            }
            int_keys.sort_by_key(|(n, _)| *n);
            let mut out: Vec<i64> = Vec::with_capacity(int_keys.len() + str_keys.len());
            for (_, k) in int_keys {
                out.push(alloc_entry(Entry::String(k.into_bytes())) as i64);
            }
            for k in str_keys {
                out.push(alloc_entry(Entry::String(k.into_bytes())) as i64);
            }
            out
        }
        Some(Entry::Vec(v)) => {
            // (cross-runtime #52/#94) JS spec: Object.keys em sparse array
            // retorna so' indices presentes. Slot `i64::MIN + 2` (undefined
            // sentinela do codegen de array literal) representa hole.
            // (cross-runtime #52/#94) JS spec: Object.keys em sparse array
            // pula holes (sentinela i64::MIN+4) mas inclui `undefined` literal
            // explicito (MIN+2).
            v.iter()
                .enumerate()
                .filter(|(_, x)| **x != i64::MIN + 4)
                .map(|(i, _)| {
                    alloc_entry(Entry::String(i.to_string().into_bytes())) as i64
                })
                .collect()
        }
        Some(Entry::String(b)) => {
            // Object.keys(new String("hi")) → ["0", "1"] — UTF-16 indices.
            let char_count = String::from_utf8_lossy(b).encode_utf16().count();
            (0..char_count)
                .map(|i| alloc_entry(Entry::String(i.to_string().into_bytes())) as i64)
                .collect()
        }
        _ => Vec::new(),
    });
    alloc_entry(Entry::Vec(Box::new(result)))
}

// (#479 follow-up) Frozen/sealed tracking global. Antes era thread-local,
// mas handles cruzam threads (HandleTable e' shard global), entao freeze
// numa thread nao tinha efeito em escritas vindas de outra. Sets globais
// resolvem o bug. Bumpar generation no free_handle invalida handles velhos
// que continuariam no set — sem aliasing falso.
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn frozen_set() -> &'static Mutex<HashSet<u64>> {
    static S: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Handles marcados como Set vs Map. Backing eh o mesmo IndexMap, mas
/// codigo que diferencia (\`Object.prototype.toString.call\`, \`typeof\`)
/// consulta esses sets.
fn map_kind_set() -> &'static Mutex<HashSet<u64>> {
    static S: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

fn set_kind_set() -> &'static Mutex<HashSet<u64>> {
    static S: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MARK_AS_MAP(handle: u64) {
    map_kind_set().lock().unwrap().insert(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MARK_AS_SET(handle: u64) {
    set_kind_set().lock().unwrap().insert(handle);
}

pub(crate) fn handle_is_map_kind(handle: u64) -> bool {
    map_kind_set().lock().unwrap().contains(&handle)
}

pub(crate) fn handle_is_set_kind(handle: u64) -> bool {
    set_kind_set().lock().unwrap().contains(&handle)
}

fn sealed_set() -> &'static Mutex<HashSet<u64>> {
    static S: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(crate) fn is_map_frozen(handle: u64) -> bool {
    frozen_set().lock().unwrap().contains(&handle)
}

pub(crate) fn is_map_sealed(handle: u64) -> bool {
    sealed_set().lock().unwrap().contains(&handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_FREEZE(handle: u64) -> u64 {
    frozen_set().lock().unwrap().insert(handle);
    sealed_set().lock().unwrap().insert(handle);
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_SEAL(handle: u64) -> u64 {
    sealed_set().lock().unwrap().insert(handle);
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_IS_FROZEN(handle: u64) -> i64 {
    if is_map_frozen(handle) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_IS_SEALED(handle: u64) -> i64 {
    if is_map_sealed(handle) { 1 } else { 0 }
}

/// (#208) `Object.getPrototypeOf(obj)` — retorna handle de `__proto__` se setado
/// explicitamente. Senao, retorna sentinel `[Object.prototype]` ou
/// `[Array.prototype]` para Map/Vec respectivamente — bate com codegen
/// que materializa `Object.prototype` como handle de string.
/// Para Map criado via `Object.create(null)` o slot existe e retorna 0 (null).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO(handle: u64) -> u64 {
    // (#218 phase2) Proxy: trap `getPrototypeOf` ou forward.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_get_proto(target, handler);
    }
    // Vec → Array.prototype sentinel.
    let is_vec = with_entry(handle, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        return alloc_entry(Entry::String(b"[Array.prototype]".to_vec()));
    }
    // Map: se __proto__ slot setado explicit (por Object.create), retorna ele.
    // Se slot ausente, default Object.prototype sentinel.
    let proto_slot: Option<i64> = with_map(handle, None, |m| {
        if m.contains_key("__proto__") {
            Some(m.get("__proto__").copied().unwrap_or(0))
        } else {
            None
        }
    });
    match proto_slot {
        Some(v) => v as u64,
        None => alloc_entry(Entry::String(b"[Object.prototype]".to_vec())),
    }
}

/// Set global de (handle, key) marcados como nao-enumeravel via
/// `Object.defineProperty(obj, key, { enumerable: false, ... })`. Usado
/// por Object.keys/values/entries e for-in para filtrar.
fn non_enumerable_set() -> &'static Mutex<HashSet<(u64, String)>> {
    static S: OnceLock<Mutex<HashSet<(u64, String)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(crate) fn is_non_enumerable(handle: u64, key: &str) -> bool {
    non_enumerable_set()
        .lock()
        .unwrap()
        .contains(&(handle, key.to_string()))
}

/// (cross-runtime #795) Tracking de writable=false definido via
/// Object.defineProperty / Reflect.defineProperty. Usado pelo
/// `Reflect.getOwnPropertyDescriptor` pra retornar writable correto.
fn non_writable_set() -> &'static Mutex<HashSet<(u64, String)>> {
    static S: OnceLock<Mutex<HashSet<(u64, String)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(crate) fn is_non_writable(handle: u64, key: &str) -> bool {
    non_writable_set()
        .lock()
        .unwrap()
        .contains(&(handle, key.to_string()))
}

pub(crate) fn mark_non_writable(handle: u64, key: &str) {
    non_writable_set()
        .lock()
        .unwrap()
        .insert((handle, key.to_string()));
}

pub(crate) fn unmark_non_writable(handle: u64, key: &str) {
    non_writable_set()
        .lock()
        .unwrap()
        .remove(&(handle, key.to_string()));
}

pub(crate) fn mark_non_enumerable(handle: u64, key: &str) {
    non_enumerable_set()
        .lock()
        .unwrap()
        .insert((handle, key.to_string()));
}

pub(crate) fn unmark_non_enumerable(handle: u64, key: &str) {
    non_enumerable_set()
        .lock()
        .unwrap()
        .remove(&(handle, key.to_string()));
}

/// (#208) `Object.defineProperty(obj, key, descriptor)`.
/// Suporta `{ value: x }` e `{ enumerable: false }`. Demais
/// (get/set/writable/configurable) caem em PR separada.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY(
    obj: u64,
    key_ptr: *const u8,
    key_len: i64,
    descriptor: u64,
) -> u64 {
    let value: i64 = with_map(descriptor, 0, |m| m.get("value").copied().unwrap_or(0));
    // enumerable: descritor com "enumerable" = 0 (bool false) marca a key
    // como nao-enumeravel. Default JS quando ausente eh false; mas pra
    // compat com codigo TS comum (object literal padrao tem enumerable=true)
    // RTS so' marca quando explicitamente false.
    // Bool eh empacotado em object literal via sentinela i64::MIN (false)
    // / i64::MIN+1 (true) pelo codegen. Strict check para evitar false
    // positive de handle nao-zero.
    let is_enumerable: Option<bool> = with_map(descriptor, None, |m| {
        m.get("enumerable").map(|v| {
            if *v == i64::MIN { false }
            else if *v == i64::MIN + 1 { true }
            else { *v != 0 }
        })
    });
    let key_str = match str_from_abi(key_ptr, key_len) {
        Some(s) => s.to_string(),
        None => return obj,
    };
    if matches!(is_enumerable, Some(false)) {
        non_enumerable_set()
            .lock()
            .unwrap()
            .insert((obj, key_str.clone()));
    } else {
        // Se redefiniu com enumerable=true (ou ausente, default keep),
        // remove marca antiga.
        non_enumerable_set()
            .lock()
            .unwrap()
            .remove(&(obj, key_str.clone()));
    }
    __RTS_FN_NS_COLLECTIONS_MAP_SET(obj, key_ptr, key_len, value);
    obj
}

/// (#678/90) Set.prototype.union(other) — ES2025. Retorna novo Set com
/// todos os elementos de `self` mais os de `other`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_UNION(self_h: u64, other_h: u64) -> u64 {
    let result = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    set_kind_set().lock().unwrap().insert(result);
    for h in [self_h, other_h] {
        let pairs: Vec<(String, i64)> = with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.iter()
                .filter(|(k, _)| k.as_str() != "__proto__")
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            _ => Vec::new(),
        });
        with_map_mut(result, (), |m| {
            for (k, v) in pairs {
                m.insert(k, v);
            }
        });
    }
    result
}

/// (#678/90) Set.prototype.intersection(other) — apenas elementos em ambos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_INTERSECTION(self_h: u64, other_h: u64) -> u64 {
    let other_keys: std::collections::HashSet<String> = with_entry(other_h, |e| match e {
        Some(Entry::Map(m)) => m.keys().filter(|k| k.as_str() != "__proto__").cloned().collect(),
        _ => std::collections::HashSet::new(),
    });
    let pairs: Vec<(String, i64)> = with_entry(self_h, |e| match e {
        Some(Entry::Map(m)) => m.iter()
            .filter(|(k, _)| k.as_str() != "__proto__" && other_keys.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    let result = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    set_kind_set().lock().unwrap().insert(result);
    with_map_mut(result, (), |m| {
        for (k, v) in pairs {
            m.insert(k, v);
        }
    });
    result
}

/// (#678/90) Set.prototype.difference(other) — elementos em self que nao estao em other.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_DIFFERENCE(self_h: u64, other_h: u64) -> u64 {
    let other_keys: std::collections::HashSet<String> = with_entry(other_h, |e| match e {
        Some(Entry::Map(m)) => m.keys().filter(|k| k.as_str() != "__proto__").cloned().collect(),
        _ => std::collections::HashSet::new(),
    });
    let pairs: Vec<(String, i64)> = with_entry(self_h, |e| match e {
        Some(Entry::Map(m)) => m.iter()
            .filter(|(k, _)| k.as_str() != "__proto__" && !other_keys.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    let result = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    set_kind_set().lock().unwrap().insert(result);
    with_map_mut(result, (), |m| {
        for (k, v) in pairs {
            m.insert(k, v);
        }
    });
    result
}

/// (#678/302) Set.symmetricDifference(other) — elementos em um Set ou outro mas nao em ambos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_SYMMETRIC_DIFFERENCE(self_h: u64, other_h: u64) -> u64 {
    let self_keys: Vec<(String, i64)> = with_entry(self_h, |e| match e {
        Some(Entry::Map(m)) => m.iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    let other_keys: Vec<(String, i64)> = with_entry(other_h, |e| match e {
        Some(Entry::Map(m)) => m.iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    let self_set: std::collections::HashSet<&String> = self_keys.iter().map(|(k, _)| k).collect();
    let other_set: std::collections::HashSet<&String> = other_keys.iter().map(|(k, _)| k).collect();
    let result = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    set_kind_set().lock().unwrap().insert(result);
    with_map_mut(result, (), |m| {
        for (k, v) in &self_keys {
            if !other_set.contains(k) { m.insert(k.clone(), *v); }
        }
        for (k, v) in &other_keys {
            if !self_set.contains(k) { m.insert(k.clone(), *v); }
        }
    });
    result
}

/// (#678/302) Set.isSubsetOf(other) — todos elementos de self estao em other.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_IS_SUBSET(self_h: u64, other_h: u64) -> i64 {
    let other_keys: std::collections::HashSet<String> = with_entry(other_h, |e| match e {
        Some(Entry::Map(m)) => m.keys().filter(|k| k.as_str() != "__proto__").cloned().collect(),
        _ => std::collections::HashSet::new(),
    });
    let all_in: bool = with_entry(self_h, |e| match e {
        Some(Entry::Map(m)) => m.keys()
            .filter(|k| k.as_str() != "__proto__")
            .all(|k| other_keys.contains(k.as_str())),
        _ => true,
    });
    if all_in { 1 } else { 0 }
}

/// (#678/302) Set.isSupersetOf(other) — todos elementos de other estao em self.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_IS_SUPERSET(self_h: u64, other_h: u64) -> i64 {
    __RTS_FN_NS_COLLECTIONS_SET_IS_SUBSET(other_h, self_h)
}

/// (#678/302) Set.isDisjointFrom(other) — nenhum elemento em comum.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_IS_DISJOINT(self_h: u64, other_h: u64) -> i64 {
    let other_keys: std::collections::HashSet<String> = with_entry(other_h, |e| match e {
        Some(Entry::Map(m)) => m.keys().filter(|k| k.as_str() != "__proto__").cloned().collect(),
        _ => std::collections::HashSet::new(),
    });
    let any_in: bool = with_entry(self_h, |e| match e {
        Some(Entry::Map(m)) => m.keys()
            .filter(|k| k.as_str() != "__proto__")
            .any(|k| other_keys.contains(k.as_str())),
        _ => false,
    });
    if any_in { 0 } else { 1 }
}

/// (#678/89) Object.groupBy(arr, fn) — ES2024. Itera arr, chama fn(elem, idx)
/// para obter key. Cria obj { key: [items...] } com todos elements agrupados.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_OBJECT_GROUP_BY(arr: u64, fn_ptr: u64) -> u64 {
    group_by_impl(arr, fn_ptr, false)
}

/// (#678/89) Map.groupBy(arr, fn) — igual ao Object.groupBy mas marca resultado
/// como Map (Array.from(map.entries()) retorna pairs).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GROUP_BY(arr: u64, fn_ptr: u64) -> u64 {
    group_by_impl(arr, fn_ptr, true)
}

fn group_by_impl(arr: u64, fn_ptr: u64, mark_as_map: bool) -> u64 {
    let items: Vec<i64> = with_entry(arr, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let result = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    if mark_as_map {
        map_kind_set().lock().unwrap().insert(result);
    }
    if fn_ptr == 0 || items.is_empty() {
        return result;
    }
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    for (i, &item) in items.iter().enumerate() {
        let key_h = f(item, i as i64);
        // Resolve key: pode ser handle de string OU int direto.
        let key_str: String = if key_h > 0xFFFF_FFFF {
            with_entry(key_h as u64, |e| match e {
                Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                _ => key_h.to_string(),
            })
        } else {
            key_h.to_string()
        };
        // Pega ou cria Vec naquele key.
        let existing_vec_h: i64 = with_map(result, 0, |m| m.get(&key_str).copied().unwrap_or(0));
        let vec_h = if existing_vec_h == 0 {
            let h = alloc_entry(Entry::Vec(Box::new(Vec::new()))) as i64;
            with_map_mut(result, (), |m| {
                m.insert(key_str, h);
            });
            h
        } else {
            existing_vec_h
        };
        with_entry_mut(vec_h as u64, |e| {
            if let Some(Entry::Vec(v)) = e {
                v.push(item);
            }
        });
    }
    result
}

/// (#208 / #479) `Object.fromEntries(iter)` — aceita:
/// - Vec de pares [key_handle, value] (Object.entries output, ou tuples literais)
/// - Map direto (cada entry vira prop com mesma key/value)
/// (#666/265) Map suporte adicionado para `Object.fromEntries(new Map(...))`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES(arr: u64) -> u64 {
    use crate::namespaces::gc::handles::{Entry, with_entry};
    // (#666/265) Map -> Map roundtrip: copia todos os pares (k, v) direto.
    let map_pairs: Option<Vec<(String, i64)>> = with_entry(arr, |entry| match entry {
        Some(Entry::Map(m)) => Some(m.iter().map(|(k, v)| (k.clone(), *v)).collect()),
        _ => None,
    });
    if let Some(pairs) = map_pairs {
        let m = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
        with_map_mut(m, (), |mm| {
            for (k, v) in pairs {
                mm.insert(k, v);
            }
        });
        return m;
    }
    let pair_handles: Vec<i64> = with_entry(arr, |entry| match entry {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let m = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    for ph in pair_handles {
        let pair_h = ph as u64;
        let kv: Option<(String, i64)> = with_entry(pair_h, |entry| match entry {
            Some(Entry::Vec(pair)) if pair.len() >= 2 => {
                let key_h = pair[0] as u64;
                let key_str: Option<String> = with_entry(key_h, |ke| match ke {
                    Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                    _ => None,
                });
                key_str.map(|k| (k, pair[1]))
            }
            _ => None,
        });
        if let Some((k, v)) = kv {
            with_map_mut(m, (), |mm| {
                mm.insert(k, v);
            });
        }
    }
    m
}

#[cfg(test)]
mod object_tests {
    use super::*;
    use crate::namespaces::gc::handles::{Entry, with_entry};

    fn read_str(h: u64) -> Option<String> {
        with_entry(h, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        })
    }

    fn read_vec(h: u64) -> Vec<i64> {
        with_entry(h, |e| match e {
            Some(Entry::Vec(v)) => v.as_ref().clone(),
            _ => Vec::new(),
        })
    }

    fn map_with(pairs: &[(&str, i64)]) -> u64 {
        let h = __RTS_FN_NS_COLLECTIONS_MAP_NEW();
        for (k, v) in pairs {
            with_map_mut(h, (), |m| {
                m.insert((*k).to_string(), *v);
            });
        }
        h
    }

    #[test]
    fn entries_returns_pairs_sorted() {
        let m = map_with(&[("b", 2), ("a", 1), ("c", 3)]);
        let entries = __RTS_FN_NS_COLLECTIONS_MAP_ENTRIES(m);
        let outer = read_vec(entries);
        assert_eq!(outer.len(), 3);
        let p0 = read_vec(outer[0] as u64);
        assert_eq!(read_str(p0[0] as u64).unwrap(), "a");
        assert_eq!(p0[1], 1);
    }

    #[test]
    fn assign_copies_source_to_target() {
        let target = map_with(&[("x", 1)]);
        let source = map_with(&[("y", 2), ("z", 3)]);
        let result = __RTS_FN_NS_COLLECTIONS_MAP_ASSIGN(target, source);
        assert_eq!(result, target);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_MAP_LEN(target), 3);
    }

    #[test]
    fn from_entries_roundtrip() {
        let m = map_with(&[("a", 1), ("b", 2)]);
        let entries = __RTS_FN_NS_COLLECTIONS_MAP_ENTRIES(m);
        let back = __RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES(entries);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_MAP_LEN(back), 2);
    }

    #[test]
    fn freeze_returns_same_handle() {
        let m = map_with(&[("a", 1)]);
        assert_eq!(__RTS_FN_NS_COLLECTIONS_MAP_FREEZE(m), m);
    }

    #[test]
    fn frozen_state_is_visible_across_threads() {
        let m = map_with(&[("x", 1)]);
        __RTS_FN_NS_COLLECTIONS_MAP_FREEZE(m);
        let visible = std::thread::spawn(move || is_map_frozen(m))
            .join()
            .unwrap();
        assert!(visible, "frozen flag must be visible from other threads");
    }

    #[test]
    fn sealed_blocks_set_from_other_thread() {
        let m = map_with(&[("x", 1)]);
        __RTS_FN_NS_COLLECTIONS_MAP_SEAL(m);
        std::thread::spawn(move || {
            let key = b"new_key";
            __RTS_FN_NS_COLLECTIONS_MAP_SET(m, key.as_ptr(), key.len() as i64, 99);
        })
        .join()
        .unwrap();
        assert_eq!(__RTS_FN_NS_COLLECTIONS_MAP_LEN(m), 1);
    }
}
