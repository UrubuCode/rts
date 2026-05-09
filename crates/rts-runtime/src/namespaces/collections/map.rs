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
/// `Object.create(null)` — Map sem chain.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_CREATE(proto: u64) -> u64 {
    let h = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    if proto != 0 {
        with_map_mut(h, (), |m| {
            m.insert("__proto__".to_string(), proto as i64);
        });
    }
    h
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
    with_map(handle, 0, |m| if m.contains_key(key) { 1 } else { 0 })
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
        // (#208) Filtra `__proto__` — JS spec: Object.keys retorna so
        // own enumeravel, e __proto__ nao deve aparecer em iteracao.
        let mut ks: Vec<String> = m.keys()
            .filter(|k| k.as_str() != "__proto__")
            .cloned()
            .collect();
        ks.sort();
        ks
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

/// (#266) Object.values(obj) — retorna Vec<i64> com valores. Ordem por
/// keys sorted asc.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_VALUES(handle: u64) -> u64 {
    let vals: Vec<i64> = with_map(handle, Vec::new(), |m| {
        // (#208) Filtra `__proto__` — JS spec.
        let mut entries: Vec<(&String, &i64)> = m.iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries.into_iter().map(|(_, v)| *v).collect()
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
        // (#208) Filtra `__proto__` — JS spec.
        let mut entries: Vec<(String, i64)> = m.iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
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

/// Object.keys auto: se handle e' Map retorna keys; se Vec retorna ["0","1",...].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_OBJECT_KEYS_AUTO(handle: u64) -> u64 {
    // (#218 phase2) Proxy: trap `ownKeys(target)` ou forward.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_own_keys(target, handler);
    }
    let result: Vec<i64> = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => {
            m.keys()
                .filter(|k| k.as_str() != "__proto__")
                .map(|k| {
                    alloc_entry(Entry::String(k.as_bytes().to_vec())) as i64
                })
                .collect()
        }
        Some(Entry::Vec(v)) => {
            (0..v.len())
                .map(|i| {
                    alloc_entry(Entry::String(i.to_string().into_bytes())) as i64
                })
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

/// (#208) `Object.getPrototypeOf(obj)` — retorna handle de `__proto__` ou 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO(handle: u64) -> u64 {
    // (#218 phase2) Proxy: trap `getPrototypeOf` ou forward.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_get_proto(target, handler);
    }
    let proto: i64 = with_map(handle, 0, |m| m.get("__proto__").copied().unwrap_or(0));
    proto as u64
}

/// (#208) `Object.defineProperty(obj, key, descriptor)` — v0 simples.
/// Suporta apenas `{ value: x }`. Demais (get/set/writable/enumerable)
/// caem em PR separada.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY(
    obj: u64,
    key_ptr: *const u8,
    key_len: i64,
    descriptor: u64,
) -> u64 {
    let value: i64 = with_map(descriptor, 0, |m| m.get("value").copied().unwrap_or(0));
    __RTS_FN_NS_COLLECTIONS_MAP_SET(obj, key_ptr, key_len, value);
    obj
}

/// (#208 / #479) `Object.fromEntries(arr)` — recebe Vec de pares
/// [key_handle, value] e cria Map. Inverso de Object.entries.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES(arr: u64) -> u64 {
    use crate::namespaces::gc::handles::{Entry, with_entry};
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
