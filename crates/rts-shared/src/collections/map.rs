//! IndexMap<String, i64> — mapa de chave string para valor i64.
//!
//! Usa `indexmap::IndexMap` para preservar ordem de inserção, necessário
//! para implementar a ordem de enumeração de propriedades do JS:
//! - integer-indexed keys (`"0"`, `"1"`, `"2"`, ...) em ordem numérica
//!   ascendente;
//! - demais string keys em ordem de inserção.

use indexmap::IndexMap;

use rts_engine::abi::ty::{Bool, Handle, I64, U64};
use rts_engine::{AbiType, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry, with_entry_mut};
// Fonte única desses helpers (antes duplicados aqui e em
// `rts-primitives::object`, que não pode depender desta crate — seria ciclo).
pub(crate) use rts_engine::heap::mapkey::with_map_mut;
use rts_engine::heap::mapkey::{
    decode_symbol_key, is_symbol_key, parse_array_index, str_from_abi, with_map,
};

/// (#216/299) True se a key `@@sym:<h>` referencia o well-known
/// `Symbol.iterator` (handle sticky cacheado em globals/symbol/rt).
fn key_is_well_known_iterator(k: &str) -> bool {
    match decode_symbol_key(k) {
        Some(h) => h == rts_primitives::symbol::__RTS_FN_GL_SYMBOL_ITERATOR(),
        None => false,
    }
}

/// Membros Map do namespace `collections` (parte; vec.rs declara a outra).
/// Hand-written (Fase 2, remoção da `rts-macro`): cada membro é um extern "C"
/// + uma entrada em `append_engine_members`, agregada pelo `register()` do
/// owner (`collections::mod`). As fns nao-membro (`MAP_GET_CHAIN`/
/// `MAP_GET_PROTO`/`MAP_DEFINE_PROPERTY`/`__RTS_FN_RT_MAP_*`/helpers) ficam
/// como free fns abaixo. (A família `*_AUTO` foi deletada no drain N7 de
/// 2026-07-31 — nenhuma tinha consumidor.)
/// (cross-runtime #1533) Membros herdados de Object.prototype — presentes
/// via `in` em qualquer objeto/array.
fn is_object_proto_member(key: &str) -> bool {
    matches!(
        key,
        "toString"
            | "hasOwnProperty"
            | "valueOf"
            | "isPrototypeOf"
            | "propertyIsEnumerable"
            | "toLocaleString"
            | "constructor"
    )
}

/// (cross-runtime #1533) Membros herdados de Array.prototype.
fn is_array_proto_member(key: &str) -> bool {
    matches!(
        key,
        "push"
            | "pop"
            | "shift"
            | "unshift"
            | "slice"
            | "splice"
            | "concat"
            | "join"
            | "reverse"
            | "sort"
            | "indexOf"
            | "lastIndexOf"
            | "includes"
            | "find"
            | "findIndex"
            | "findLast"
            | "findLastIndex"
            | "map"
            | "filter"
            | "forEach"
            | "reduce"
            | "reduceRight"
            | "every"
            | "some"
            | "flat"
            | "flatMap"
            | "fill"
            | "copyWithin"
            | "keys"
            | "values"
            | "entries"
            | "at"
            | "with"
            | "toSorted"
            | "toReversed"
            | "toSpliced"
    )
}

/// HOOK shape-object (instalado pelo rts-runtime no bootstrap do motor): um
/// handle que NÃO é `Entry::Map` mas um OBJETO shape-vec do motor novo (uma
/// instância de classe/fn-ctor cujo `this` chega à superfície de baixo nível
/// `collections.map_*(this, …)` — #264) roteia pelo get/set shape-aware dos
/// adapters (obj_get / obj_set com transition) em vez de virar no-op/0.
/// `(get, set)`. rts-shared não pode depender de rts-runtime (direção de
/// deps), daí o registro dinâmico — o mesmo padrão do async-error hook.
#[allow(clippy::type_complexity)]
static SHAPED_OBJ_HOOK: std::sync::OnceLock<(
    extern "C" fn(u64, *const u8, i64) -> i64,
    extern "C" fn(u64, *const u8, i64, i64),
)> = std::sync::OnceLock::new();

/// Registra a ponte shape-object (chamado pelo rts-runtime no bootstrap).
pub fn set_shaped_object_hook(
    get: extern "C" fn(u64, *const u8, i64) -> i64,
    set: extern "C" fn(u64, *const u8, i64, i64),
) {
    let _ = SHAPED_OBJ_HOOK.set((get, set));
}

/// Se `h` é um OBJETO shape-vec (Entry::Vec, não Map) e o hook está instalado,
/// devolve a ponte; senão `None` (o caminho Map normal segue).
fn shaped_object_route(
    h: u64,
) -> Option<(
    extern "C" fn(u64, *const u8, i64) -> i64,
    extern "C" fn(u64, *const u8, i64, i64),
)> {
    let hook = SHAPED_OBJ_HOOK.get()?;
    let is_vec = with_entry(h, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec { Some(*hook) } else { None }
}

/// Creates an empty HashMap<string, number> and returns its handle.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_NEW")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_NEW() -> Handle {
    alloc_entry(Entry::Map(Box::new(IndexMap::new())))
}

/// Releases the map handle.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_FREE")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_FREE(h: U64) {
    free_handle(h);
}

/// Number of entries; -1 if the handle is invalid.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_LEN")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_LEN(h: U64) -> I64 {
    with_map(h, -1, |m| m.len() as i64)
}

/// True when the map contains `key`.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_HAS")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_HAS(
    h: U64,
    key_ptr: u64,
    key_len: i64,
) -> Bool {
    let key_ptr = key_ptr as *const u8;

    let key = match unsafe { rts_engine::abi::str_abi::from_abi(key_ptr, key_len) } {
        Some(s) => s,
        None => return 0,
    };
    // (#218) Proxy: trap `has(target, prop)` ou forward.
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(h) {
        return rts_primitives::proxy::ops::dispatch_has(target, handler, key);
    }
    with_map(h, 0, |m| if m.contains_key(key) { 1 } else { 0 })
}

/// Value for `key`, or 0 when absent. Use map_has to distinguish.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_GET")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_GET(h: U64, key_ptr: u64, key_len: i64) -> I64 {
    let key_ptr = key_ptr as *const u8;

    let key = match unsafe { rts_engine::abi::str_abi::from_abi(key_ptr, key_len) } {
        Some(s) => s,
        None => return 0,
    };
    // (#218) Proxy: dispatch get trap quando handle eh Proxy.
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(h) {
        return rts_primitives::proxy::ops::dispatch_get(target, handler, key);
    }
    // (#264) Um OBJETO shape-vec do motor novo: leitura shape-aware via hook.
    if let Some((get, _)) = shaped_object_route(h) {
        return get(h, key_ptr, key_len);
    }
    with_map(h, 0, |m| m.get(key).copied().unwrap_or(0))
}

/// Inserts/overwrites `key` with `value`.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_SET")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_SET(
    h: U64,
    key_ptr: u64,
    key_len: i64,
    value: I64,
) {
    let key_ptr = key_ptr as *const u8;

    let key = match unsafe { rts_engine::abi::str_abi::from_abi(key_ptr, key_len) } {
        Some(s) => s,
        None => return,
    };
    // (#218) Proxy: trap `set(target, prop, value)` ou forward.
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(h) {
        rts_primitives::proxy::ops::dispatch_set(target, handler, key, value);
        return;
    }
    // (#264) Um OBJETO shape-vec do motor novo: escrita shape-aware via hook.
    if let Some((_, set)) = shaped_object_route(h) {
        set(h, key_ptr, key_len, value);
        return;
    }
    // (#479 follow-up) frozen impede mutacao; sealed so' impede add de novas keys.
    if is_map_frozen(h) {
        return;
    }
    // (#1073) writable=false definido via Object.defineProperty bloqueia
    // assignment silenciosamente (sloppy mode).
    if is_non_writable(h, key) {
        return;
    }
    let key_owned = key.to_string();
    let sealed = is_map_sealed(h);
    with_map_mut(h, (), |m| {
        if sealed && !m.contains_key(&key_owned) {
            return;
        }
        m.insert(key_owned, value);
    });
}

/// Removes `key`. Returns 1 if removed, 0 if absent.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_DELETE")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_DELETE(
    h: U64,
    key_ptr: u64,
    key_len: i64,
) -> I64 {
    let key_ptr = key_ptr as *const u8;

    let key = match unsafe { rts_engine::abi::str_abi::from_abi(key_ptr, key_len) } {
        Some(s) => s,
        None => return 0,
    };
    // (#218) Proxy: trap `deleteProperty(target, prop)` ou forward.
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(h) {
        return rts_primitives::proxy::ops::dispatch_delete(target, handler, key);
    }
    // sealed/frozen impedem delete.
    if is_map_sealed(h) {
        return 0;
    }
    with_map_mut(h, 0, |m| if m.shift_remove(key).is_some() { 1 } else { 0 })
}

/// Removes all entries.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_CLEAR")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_CLEAR(h: U64) {
    with_map_mut(h, (), |m| m.clear());
}

/// True when `key_h` is an own property of `obj_h` (Vec or Map). Accepts Symbol keys.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_OBJ_HAS")]
pub fn __RTS_FN_NS_COLLECTIONS_OBJ_HAS(obj_h: U64, key_h: U64) -> Bool {
    let key = match key_handle_to_string(key_h) {
        Some(s) => s,
        None => return 0,
    };
    // (cross-runtime #340) Proxy: dispatch `has` trap quando obj_h eh Proxy.
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(obj_h) {
        return rts_primitives::proxy::ops::dispatch_has(target, handler, &key);
    }
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
            } else if is_array_proto_member(&key) || is_object_proto_member(&key) {
                // (cross-runtime #1533) `"push" in []` / `"toString" in []`
                // — membros herdados de Array.prototype/Object.prototype.
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
    // (#1091) `in` walking prototype chain + class methods.
    if with_map(obj_h, 0, |m| if m.contains_key(&key) { 1 } else { 0 }) == 1 {
        return 1;
    }
    // Walk __proto__ chain.
    let mut visited: HashSet<u64> = HashSet::new();
    let mut cur = with_map(obj_h, 0i64, |m| m.get("__proto__").copied().unwrap_or(0)) as u64;
    let mut steps = 0u32;
    while cur != 0 && steps < 64 {
        steps += 1;
        if !visited.insert(cur) {
            break;
        }
        let found = with_map(cur, 0, |m| if m.contains_key(&key) { 1 } else { 0 });
        if found == 1 {
            return 1;
        }
        cur = with_map(cur, 0i64, |m| m.get("__proto__").copied().unwrap_or(0)) as u64;
    }
    // Class method: consulta global class registry pelo __rts_class.
    let class_name: Option<String> = with_entry(obj_h, |e| match e {
        Some(Entry::Map(m)) => m.get("__rts_class").and_then(|v| {
            with_entry(*v as u64, |ce| match ce {
                Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                _ => None,
            })
        }),
        _ => None,
    });
    if let Some(cls) = class_name.as_ref() {
        if class_method_registry()
            .lock()
            .unwrap()
            .get(cls)
            .map(|set| set.contains(&key))
            .unwrap_or(false)
        {
            return 1;
        }
    }
    // (cross-runtime #1533) Membros universais de Object.prototype: todo
    // objeto (Map), com ou sem __rts_class, herda toString/valueOf/etc —
    // `"toString" in obj` eh true mesmo para `Object.create({...})`.
    let is_map = with_entry(obj_h, |e| matches!(e, Some(Entry::Map(_))));
    if is_map && is_object_proto_member(&key) {
        return 1;
    }
    0
}

/// Inserts/overwrites value for key handle (String or Symbol).
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_SET_KH")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_SET_KH(obj_h: U64, key_h: U64, value: I64) {
    let Some(key) = key_handle_to_string(key_h) else {
        return;
    };
    with_map_mut(obj_h, (), |m| {
        m.insert(key, value);
    });
}

/// Sets obj[key]=value, dispatching on Vec vs Map. Accepts Symbol keys.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_OBJ_SET")]
pub fn __RTS_FN_NS_COLLECTIONS_OBJ_SET(obj_h: U64, key_h: U64, value: I64) {
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

/// Reads obj[key], dispatching on Vec vs Map. Accepts Symbol keys.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_OBJ_GET")]
pub fn __RTS_FN_NS_COLLECTIONS_OBJ_GET(obj_h: U64, key_h: U64) -> I64 {
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

/// Returns value for key handle, or 0 if absent.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_GET_KH")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_GET_KH(obj_h: U64, key_h: U64) -> I64 {
    let Some(key) = key_handle_to_string(key_h) else {
        return 0;
    };
    // (cross-runtime #340/#53) Proxy: dispatch `get` trap antes do path raw.
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(obj_h) {
        return rts_primitives::proxy::ops::dispatch_get(target, handler, &key);
    }
    // Vec path: parse key como array index.
    let is_vec = with_entry(obj_h, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        if let Some(n) = parse_array_index(&key) {
            return super::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(obj_h, n as i64);
        }
        // (#216/299) `arr[Symbol.iterator]` -> handle Function nativo.
        if key_is_well_known_iterator(&key) {
            return rts_engine::collector::generator::__rtsm_global_Array_iterator_fn() as i64;
        }
        return 0;
    }
    // Map: valor armazenado vence; fallback iterator nativo.
    let stored = with_map(obj_h, 0, |m| m.get(&key).copied().unwrap_or(0));
    if stored == 0 && key_is_well_known_iterator(&key) {
        return rts_engine::collector::generator::__rtsm_global_Array_iterator_fn() as i64;
    }
    stored
}

/// Returns a shallow copy of the map (new handle).
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_CLONE")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_CLONE(h: U64) -> Handle {
    let cloned: Option<IndexMap<String, i64>> = with_map(h, None, |m| Some(m.clone()));
    match cloned {
        Some(m) => alloc_entry(Entry::Map(Box::new(m))),
        None => 0,
    }
}

/// Returns the key at index in deterministic order (sorted). 0 if out of range.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_KEY_AT")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_KEY_AT(h: U64, idx: I64) -> Handle {
    if idx < 0 {
        return 0;
    }
    let key_opt: Option<String> = with_map(h, None, |m| {
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
        Some(s) => rts_engine::heap::string_pool::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64),
        None => 0,
    }
}

/// Returns Vec<i64> with key handles (sorted asc). Used by Object.keys.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_KEYS")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_KEYS(h: U64) -> Handle {
    // (#218 phase2) Proxy: trap `ownKeys(target)` ou forward.
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(h) {
        return rts_primitives::proxy::ops::dispatch_own_keys(target, handler);
    }
    // (#394) Set.keys() eh alias de Set.values() em JS.
    if handle_is_set_kind(h) {
        return __RTS_FN_NS_COLLECTIONS_MAP_VALUES(h);
    }
    let keys: Vec<String> = with_map(h, Vec::new(), |m| {
        let mut int_keys: Vec<(u32, String)> = Vec::new();
        let mut str_keys: Vec<String> = Vec::new();
        for k in m.keys() {
            if k.as_str() == "__proto__" {
                continue;
            }
            if k.starts_with("__get_") || k.starts_with("__set_") {
                continue;
            }
            if is_symbol_key(k.as_str()) {
                continue;
            }
            if is_non_enumerable(h, k.as_str()) {
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
        let h2 = rts_engine::heap::string_pool::__RTS_FN_NS_GC_STRING_NEW(k.as_ptr(), k.len() as i64);
        vec.push(h2 as i64);
    }
    rts_engine::heap::handles::alloc_entry(rts_engine::heap::handles::Entry::Vec(Box::new(vec)))
}

/// Returns Vec<i64> with values (in key-sorted order). Used by Object.values.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_VALUES")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_VALUES(h: U64) -> Handle {
    if handle_is_set_kind(h) {
        let elems: Vec<i64> = with_map(h, Vec::new(), |m| {
            m.iter()
                .filter(|(k, _)| k.as_str() != "__proto__")
                .map(|(k, &v)| set_element_from_pair(k, v))
                .collect()
        });
        return rts_engine::heap::handles::alloc_entry(rts_engine::heap::handles::Entry::Vec(
            Box::new(elems),
        ));
    }
    let vals: Vec<i64> = with_map(h, Vec::new(), |m| {
        let mut int_entries: Vec<(u32, i64)> = Vec::new();
        let mut str_entries: Vec<i64> = Vec::new();
        for (k, v) in m.iter() {
            if k.as_str() == "__proto__" {
                continue;
            }
            if is_symbol_key(k.as_str()) {
                continue;
            }
            if is_non_enumerable(h, k.as_str()) {
                continue;
            }
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
    rts_engine::heap::handles::alloc_entry(rts_engine::heap::handles::Entry::Vec(Box::new(vals)))
}

/// Constrói um `Member` (formato builder do `rts-engine`) para uma função do
/// namespace `collections`. Helper local — substitui o `func` que a macro
/// `#[rts_namespace(part)]` gerava implicitamente.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Empurra os membros Map desta `part` (formato builder do `rts-engine`) na
/// lista do owner. Chamado pelo `register()` agregador de `collections::mod`,
/// ANTES de `vec::append_engine_members` — preserva a ordem map→vec do antigo
/// `concat_members`. Hand-written (Fase 2, remoção da `rts-macro`).
pub fn append_engine_members(v: &mut Vec<Member>) {
    v.push(func(
        "map_new",
        "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
        Sig::new(Vec::new(), AbiType::Handle),
        "map_new(): number",
        "Creates an empty HashMap<string, number> and returns its handle.",
        __RTS_FN_NS_COLLECTIONS_MAP_NEW as *const u8,
    ));
    v.push(func(
        "map_free",
        "__RTS_FN_NS_COLLECTIONS_MAP_FREE",
        Sig::new(vec![AbiType::U64], AbiType::Void),
        "map_free(h: number): void",
        "Releases the map handle.",
        __RTS_FN_NS_COLLECTIONS_MAP_FREE as *const u8,
    ));
    v.push(func(
        "map_len",
        "__RTS_FN_NS_COLLECTIONS_MAP_LEN",
        Sig::new(vec![AbiType::U64], AbiType::I64),
        "map_len(h: number): number",
        "Number of entries; -1 if the handle is invalid.",
        __RTS_FN_NS_COLLECTIONS_MAP_LEN as *const u8,
    ));
    v.push(func(
        "map_has",
        "__RTS_FN_NS_COLLECTIONS_MAP_HAS",
        Sig::new(vec![AbiType::U64, AbiType::StrPtr], AbiType::Bool),
        "map_has(h: number, key: string): boolean",
        "True when the map contains `key`.",
        __RTS_FN_NS_COLLECTIONS_MAP_HAS as *const u8,
    ));
    v.push(func(
        "map_get",
        "__RTS_FN_NS_COLLECTIONS_MAP_GET",
        Sig::new(vec![AbiType::U64, AbiType::StrPtr], AbiType::I64),
        "map_get(h: number, key: string): number",
        "Value for `key`, or 0 when absent. Use map_has to distinguish.",
        __RTS_FN_NS_COLLECTIONS_MAP_GET as *const u8,
    ));
    v.push(func(
        "map_set",
        "__RTS_FN_NS_COLLECTIONS_MAP_SET",
        Sig::new(
            vec![AbiType::U64, AbiType::StrPtr, AbiType::I64],
            AbiType::Void,
        ),
        "map_set(h: number, key: string, value: number): void",
        "Inserts/overwrites `key` with `value`.",
        __RTS_FN_NS_COLLECTIONS_MAP_SET as *const u8,
    ));
    v.push(func(
        "map_delete",
        "__RTS_FN_NS_COLLECTIONS_MAP_DELETE",
        Sig::new(vec![AbiType::U64, AbiType::StrPtr], AbiType::I64),
        "map_delete(h: number, key: string): number",
        "Removes `key`. Returns 1 if removed, 0 if absent.",
        __RTS_FN_NS_COLLECTIONS_MAP_DELETE as *const u8,
    ));
    v.push(func(
        "map_clear",
        "__RTS_FN_NS_COLLECTIONS_MAP_CLEAR",
        Sig::new(vec![AbiType::U64], AbiType::Void),
        "map_clear(h: number): void",
        "Removes all entries.",
        __RTS_FN_NS_COLLECTIONS_MAP_CLEAR as *const u8,
    ));
    v.push(func(
        "obj_has",
        "__RTS_FN_NS_COLLECTIONS_OBJ_HAS",
        Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::Bool),
        "obj_has(obj_h: number, key_h: number): boolean",
        "True when `key_h` is an own property of `obj_h` (Vec or Map). Accepts Symbol keys.",
        __RTS_FN_NS_COLLECTIONS_OBJ_HAS as *const u8,
    ));
    v.push(func(
        "map_set_kh",
        "__RTS_FN_NS_COLLECTIONS_MAP_SET_KH",
        Sig::new(
            vec![AbiType::U64, AbiType::U64, AbiType::I64],
            AbiType::Void,
        ),
        "map_set_kh(obj_h: number, key_h: number, value: number): void",
        "Inserts/overwrites value for key handle (String or Symbol).",
        __RTS_FN_NS_COLLECTIONS_MAP_SET_KH as *const u8,
    ));
    v.push(func(
        "obj_set",
        "__RTS_FN_NS_COLLECTIONS_OBJ_SET",
        Sig::new(
            vec![AbiType::U64, AbiType::U64, AbiType::I64],
            AbiType::Void,
        ),
        "obj_set(obj_h: number, key_h: number, value: number): void",
        "Sets obj[key]=value, dispatching on Vec vs Map. Accepts Symbol keys.",
        __RTS_FN_NS_COLLECTIONS_OBJ_SET as *const u8,
    ));
    v.push(func(
        "obj_get",
        "__RTS_FN_NS_COLLECTIONS_OBJ_GET",
        Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::I64),
        "obj_get(obj_h: number, key_h: number): number",
        "Reads obj[key], dispatching on Vec vs Map. Accepts Symbol keys.",
        __RTS_FN_NS_COLLECTIONS_OBJ_GET as *const u8,
    ));
    v.push(func(
        "map_get_kh",
        "__RTS_FN_NS_COLLECTIONS_MAP_GET_KH",
        Sig::new(vec![AbiType::U64, AbiType::U64], AbiType::I64),
        "map_get_kh(obj_h: number, key_h: number): number",
        "Returns value for key handle, or 0 if absent.",
        __RTS_FN_NS_COLLECTIONS_MAP_GET_KH as *const u8,
    ));
    v.push(func(
        "map_clone",
        "__RTS_FN_NS_COLLECTIONS_MAP_CLONE",
        Sig::new(vec![AbiType::U64], AbiType::Handle),
        "map_clone(h: number): number",
        "Returns a shallow copy of the map (new handle).",
        __RTS_FN_NS_COLLECTIONS_MAP_CLONE as *const u8,
    ));
    v.push(func(
        "map_key_at",
        "__RTS_FN_NS_COLLECTIONS_MAP_KEY_AT",
        Sig::new(vec![AbiType::U64, AbiType::I64], AbiType::Handle),
        "map_key_at(h: number, idx: number): number",
        "Returns the key at index in deterministic order (sorted). 0 if out of range.",
        __RTS_FN_NS_COLLECTIONS_MAP_KEY_AT as *const u8,
    ));
    v.push(func(
        "map_keys",
        "__RTS_FN_NS_COLLECTIONS_MAP_KEYS",
        Sig::new(vec![AbiType::U64], AbiType::Handle),
        "map_keys(h: number): number",
        "Returns Vec<i64> with key handles (sorted asc). Used by Object.keys.",
        __RTS_FN_NS_COLLECTIONS_MAP_KEYS as *const u8,
    ));
    v.push(func(
        "map_values",
        "__RTS_FN_NS_COLLECTIONS_MAP_VALUES",
        Sig::new(vec![AbiType::U64], AbiType::Handle),
        "map_values(h: number): number",
        "Returns Vec<i64> with values (in key-sorted order). Used by Object.values.",
        __RTS_FN_NS_COLLECTIONS_MAP_VALUES as *const u8,
    ));
}

// The `__RTS_FN_GL_OBJECT_*` bodies and the `null_proto_set` that used to be
// re-exported here are GONE — every one of them was dead, superseded by the
// `__rtsadp_obj_*` trampolines on the shape-based value model. A null prototype
// is recorded as the explicit `0` sentinel in the `__proto__` slot, which the
// code below and `rts-std`'s inspector already read directly. See
// `rts-primitives/src/object/mod.rs` for the full account of the drain.

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

/// (#1114) Registry de methods declarados por classe; consultado pelo `in`
/// operator (OBJ_HAS).
///
/// (N7 dead-symbol drain, 2026-07-31) O único escritor era
/// `__RTS_FN_NS_COLLECTIONS_REGISTER_CLASS_METHOD`, que o codegen deixou de
/// emitir — o símbolo foi deletado por não ter consumidor. O registry
/// permanece porque `OBJ_HAS` o consulta, mas hoje está sempre vazio: `in`
/// resolve métodos de classe pelos outros caminhos (proto chain +
/// `is_object_proto_member`). Reintroduzir a marcação exige um escritor novo,
/// não a ressurreição do símbolo morto.
fn class_method_registry()
-> &'static std::sync::Mutex<std::collections::HashMap<String, std::collections::HashSet<String>>> {
    static R: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, std::collections::HashSet<String>>>,
    > = std::sync::OnceLock::new();
    R.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

// (LAYERING FIX 2026-07-24) __RTS_FN_GL_OBJECT_IS_PROTOTYPE_OF/
// PROPERTY_IS_ENUMERABLE MUDARAM para `rts-primitives::object` (Object é
// PRIMORDIAL).

/// (#264 PR4) Retorna valor de `key` seguindo cadeia `__proto__`.
/// Se a key nao existe no map, le `__proto__` (handle de outro Map) e
/// recursa. Retorna 0 quando atinge o fim da cadeia ou tipo invalido.
/// Guard contra ciclos: profundidade maxima 64.
///
/// Codifica o estado de busca em 2 valores: -1 = nao tem proto (parar),
/// 0 = nao tem key mas tem proto (continuar), >0 = tem key (retornar).
/// Na pratica nao distinguimos -1 e 0 porque ambos paramos em 0 retorno.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN(
    handle: u64,
    key_ptr: u64,
    key_len: i64,
) -> i64 {
    let key_ptr = key_ptr as *const u8;

    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    // (#218) Proxy: se handle for Entry::Proxy, dispara trap `get` no handler
    // ou faz forward para target. Trap recebe (target, key_handle).
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(handle) {
        return rts_primitives::proxy::ops::dispatch_get(target, handler, key);
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
    // (#1104) `e.constructor` em ErrorObj — JS spec: retorna ref a classe
    // (TypeError/Error/etc) cujo `.name` eh o nome do construtor. RTS
    // reify um Map sintetico com slot "name" para satisfazer o padrao
    // comum `e.constructor.name`.
    let ctor_name: Option<String> = with_entry(handle, |e| match e {
        Some(Entry::ErrorObj { name, .. }) if key == "constructor" => Some(name.clone()),
        _ => None,
    });
    if let Some(class_name) = ctor_name {
        let mut m: IndexMap<String, i64> = IndexMap::new();
        let name_h = alloc_entry(Entry::String(class_name.into_bytes())) as i64;
        m.insert("name".to_string(), name_h);
        return alloc_entry(Entry::Map(Box::new(m))) as i64;
    }
    // (cross-runtime #97) Regex instance getters via MAP_GET_CHAIN: quando
    // `obj.re.source` chega aqui com handle de Entry::Regex, dispatch para
    // o getter builtin em vez de retornar 0.
    let regex_field: Option<String> = with_entry(handle, |e| match e {
        Some(Entry::Regex(rx)) => match key {
            "source" => Some(rx.engine.source()),
            "flags" => Some(rx.flags.clone()),
            _ => None,
        },
        _ => None,
    });
    if let Some(s) = regex_field {
        return alloc_entry(Entry::String(s.into_bytes())) as i64;
    }
    // (cross-runtime #97) Regex bool getters retornam i64 sentinela
    // (MIN ou MIN+1) que TPL_COERCE_AUTO formata como "true"/"false".
    let regex_bool: Option<bool> = with_entry(handle, |e| match e {
        Some(Entry::Regex(rx)) => match key {
            "global" => Some(rx.global),
            "ignoreCase" => Some(rx.flags.contains('i')),
            "multiline" => Some(rx.flags.contains('m')),
            _ => None,
        },
        _ => None,
    });
    if let Some(b) = regex_bool {
        return if b { i64::MIN + 1 } else { i64::MIN };
    }
    // (cross-runtime #336) Entry::Function via MAP_GET_CHAIN: `cn.name` /
    // `cn.length` em handles reificados (ex: ctor stub do prototype map
    // criado em FUNCTION_PROTOTYPE_GET). Sem isso, leitura caia no
    // generic map loop que retorna 0 (Function nao tem slots Map).
    let fn_field: Option<i64> = rts_engine::heap::handles::with_entry(handle, |e| match e {
        Some(Entry::Function(d)) => match key {
            "name" => Some(alloc_entry(Entry::String(d.name.as_bytes().to_vec())) as i64),
            "length" => Some(d.arity as i64),
            _ => None,
        },
        _ => None,
    });
    if let Some(v) = fn_field {
        return v;
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

// (#479 follow-up) Frozen/sealed tracking global. Antes era thread-local,
// mas handles cruzam threads (HandleTable e' shard global), entao freeze
// numa thread nao tinha efeito em escritas vindas de outra. Sets globais
// resolvem o bug. Bumpar generation no free_handle invalida handles velhos
// que continuariam no set — sem aliasing falso.
//
// (N7 dead-symbol drain, 2026-07-31) Os ÚNICOS escritores destes dois sets
// eram `MAP_FREEZE` / `MAP_SEAL`, deletados por não terem consumidor algum.
// Os leitores (`is_map_frozen` / `is_map_sealed`, consultados por MAP_SET e
// MAP_DELETE) ficam — mas hoje os sets estão sempre vazios, ou seja
// freeze/seal desta camada é inerte. Object.freeze/seal reais passam pelos
// trampolins `__rtsadp_obj_*` do value model; se um dia o caminho Map voltar
// a precisar disso, o escritor tem de ser reintroduzido explicitamente.
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn frozen_set() -> &'static Mutex<HashSet<u64>> {
    static S: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Handles marcados como Set vs Map. Backing eh o mesmo IndexMap, mas
/// codigo que diferencia (\`Object.prototype.toString.call\`, \`typeof\`)
/// consulta esses sets.
///
/// (N7 dead-symbol drain, 2026-07-31) Os pontos de entrada de MARCAÇÃO
/// `MARK_AS_MAP` / `MARK_AS_SET` foram deletados (nenhum consumidor). As
/// TABELAS ficam obrigatoriamente: `handle_is_map_kind`/`handle_is_set_kind`
/// são chamados de FORA desta crate (`rts-std`, `rts-shared::collections::vec`)
/// e continuam sendo a leitura de verdade. Escritor remanescente: só
/// `mark_set_kind` (usado por `rts-std` no structuredClone) alimenta
/// `set_kind_set`; `map_kind_set` ficou sem escritor.
fn map_kind_set() -> &'static Mutex<HashSet<u64>> {
    static S: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

fn set_kind_set() -> &'static Mutex<HashSet<u64>> {
    static S: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn handle_is_map_kind(handle: u64) -> bool {
    map_kind_set().lock().unwrap().contains(&handle)
}

pub fn handle_is_set_kind(handle: u64) -> bool {
    set_kind_set().lock().unwrap().contains(&handle)
}

/// (#394) Recupera o ELEMENTO de um Set a partir do par (key, value) do Map
/// interno. Os escritores gravam o VALOR original do elemento como value,
/// preservando a identidade (handle de objeto/Set aninhado/string). A leitura
/// prefere esse value.
///
/// Fallback (value sentinel legado `1` E a key nao parseia como `1`): Sets
/// criados pelo caminho float (que mantem value=`1`) ou por estados antigos —
/// reconstroi o elemento parseando a key (int) ou alocando string handle.
/// Quando key=="1" o value=1 eh o numero genuino, entao usa o value direto.
pub(crate) fn set_element_from_pair(key: &str, value: i64) -> i64 {
    if value == 1 && key != "1" {
        // value sentinel legado / float: reconstroi da key.
        match key.parse::<i64>() {
            Ok(n) => n,
            Err(_) => rts_engine::heap::handles::alloc_entry(
                rts_engine::heap::handles::Entry::String(key.as_bytes().to_vec()),
            ) as i64,
        }
    } else {
        value
    }
}

/// (#316) Helper interno para estruturas que clonam (structuredClone)
/// preservarem kind=Set do source no novo handle.
pub fn mark_set_kind(handle: u64) {
    set_kind_set().lock().unwrap().insert(handle);
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

/// (#208) `Object.getPrototypeOf(obj)` — retorna handle de `__proto__` se setado
/// explicitamente. Senao, retorna sentinel `[Object.prototype]` ou
/// `[Array.prototype]` para Map/Vec respectivamente — bate com codegen
/// que materializa `Object.prototype` como handle de string.
/// Para Map criado via `Object.create(null)` o slot existe e retorna 0 (null).
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO(handle: u64) -> u64 {
    // (#218 phase2) Proxy: trap `getPrototypeOf` ou forward.
    if let Some((target, handler)) = rts_primitives::proxy::ops::resolve_proxy(handle) {
        return rts_primitives::proxy::ops::dispatch_get_proto(target, handler);
    }
    // (cross-runtime #336) Topo da prototype chain: o singleton
    // Object.prototype (Map real instalado como __proto__ raiz das classes)
    // nao tem proto — getPrototypeOf retorna null. Sem isso, iterar a chain
    // (`while (proto) { ...; proto = getPrototypeOf(proto); }`) dava uma
    // volta extra retornando o sentinel `[Object.prototype]` em vez de
    // terminar, imprimindo lixo (` 0`) apos "Object".
    if handle == rts_primitives::function::ops::__RTS_FN_RT_OBJECT_PROTOTYPE_HANDLE() {
        return 0;
    }
    // (#1080) Sentinel proto strings: JS spec:
    // - Object.getPrototypeOf(Object.prototype) === null (depth termina)
    // - Object.getPrototypeOf(Array.prototype) === Object.prototype
    // - Object.getPrototypeOf(Map.prototype) === Object.prototype
    // - Object.getPrototypeOf(Set.prototype) === Object.prototype
    let sentinel_kind: Option<&'static [u8]> = with_entry(handle, |e| match e {
        Some(Entry::String(b)) => match b.as_slice() {
            b"[Object.prototype]" => Some(b"object".as_slice()),
            b"[Array.prototype]" | b"[Map.prototype]" | b"[Set.prototype]" => {
                Some(b"chained".as_slice())
            }
            _ => None,
        },
        _ => None,
    });
    match sentinel_kind {
        Some(b"object") => return 0,
        Some(b"chained") => return alloc_entry(Entry::String(b"[Object.prototype]".to_vec())),
        _ => {}
    }
    // Vec → Array.prototype sentinel.
    let is_vec = with_entry(handle, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        return alloc_entry(Entry::String(b"[Array.prototype]".to_vec()));
    }
    // (#1080) new Map() / new Set() retornam Map/Set kind handles —
    // o proto deles eh Map.prototype / Set.prototype, nao Object.prototype.
    if handle_is_map_kind(handle) {
        return alloc_entry(Entry::String(b"[Map.prototype]".to_vec()));
    }
    if handle_is_set_kind(handle) {
        return alloc_entry(Entry::String(b"[Set.prototype]".to_vec()));
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

// (LAYERING FIX 2026-07-24) O tracking de descriptor flags (non-enumerable/
// non-writable/non-configurable) MUDOU para `rts_engine::heap::descriptors`
// — é semântica engine-level ([[DefineOwnProperty]]), não algo específico do
// backing store Map desta crate, e `reflect`/`proxy` (primordiais) precisavam
// consultar o mesmo estado sem uma dependência rts-primitives → rts-shared.
// Aliases locais preservam as chamadas existentes neste arquivo.
use rts_engine::heap::descriptors::{
    is_non_configurable, is_non_enumerable, is_non_writable, mark_non_configurable,
    mark_non_enumerable, mark_non_writable, unmark_non_writable,
};

// ── Shims extern-C p/ desacoplar `globals::function` (Fase 2.3) ───────────
// `with_map_mut`/`mark_non_enumerable` são `pub(crate)` (não atravessam crate).
// A classe primordial Function (migrada p/ `rts-primitives`) precisa dessas
// operações de prototype-map; estes shims `#[no_mangle]` as expõem por símbolo
// (resolve por link no AOT; `add_fn!` no JIT). Comportamento idêntico ao
// código inline que Function usava antes.

/// Insere `map[key] = val` (key como str ABI). Usado p/ o slot `constructor`
/// dos prototype-maps de funções-construtoras.
#[rtse::abi("__RTS_FN_RT_MAP_SET_STR")]
pub fn __RTS_FN_RT_MAP_SET_STR(map: u64, key_ptr: u64, key_len: i64, val: i64) {
    let key_ptr = key_ptr as *const u8;

    if let Some(key) = str_from_abi(key_ptr, key_len) {
        let k = key.to_string();
        with_map_mut(map, (), |m| {
            m.insert(k, val);
        });
    }
}

/// Lê `map[key]` (0 se ausente). Usado p/ andar a chain `__proto__`.
#[rtse::abi("__RTS_FN_RT_MAP_GET_STR")]
pub fn __RTS_FN_RT_MAP_GET_STR(map: u64, key_ptr: u64, key_len: i64) -> i64 {
    let key_ptr = key_ptr as *const u8;

    match str_from_abi(key_ptr, key_len) {
        Some(key) => with_map_mut(map, 0i64, |m| m.get(key).copied().unwrap_or(0)),
        None => 0,
    }
}

/// Marca `map[key]` como não-enumerável (JS: `constructor` em prototype maps).
#[rtse::abi("__RTS_FN_RT_MAP_MARK_NON_ENUM")]
pub fn __RTS_FN_RT_MAP_MARK_NON_ENUM(map: u64, key_ptr: u64, key_len: i64) {
    let key_ptr = key_ptr as *const u8;

    if let Some(key) = str_from_abi(key_ptr, key_len) {
        mark_non_enumerable(map, key);
    }
}

/// (#208) `Object.defineProperty(obj, key, descriptor)`.
/// Suporta `{ value: x }` e `{ enumerable: false }`. Demais
/// (get/set/writable/configurable) caem em PR separada.
#[rtse::abi("__RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY")]
pub fn __RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY(
    obj: u64,
    key_ptr: u64,
    key_len: i64,
    descriptor: u64,
) -> u64 {
    let key_ptr = key_ptr as *const u8;

    let value: i64 = with_map(descriptor, 0, |m| m.get("value").copied().unwrap_or(0));
    // enumerable: descritor com "enumerable" = 0 (bool false) marca a key
    // como nao-enumeravel. Default JS quando ausente eh false; mas pra
    // compat com codigo TS comum (object literal padrao tem enumerable=true)
    // RTS so' marca quando explicitamente false.
    // Bool eh empacotado em object literal via sentinela i64::MIN (false)
    // / i64::MIN+1 (true) pelo codegen. Strict check para evitar false
    // positive de handle nao-zero.
    let bool_from = |v: &i64| -> bool {
        if *v == i64::MIN {
            false
        } else if *v == i64::MIN + 1 {
            true
        } else {
            *v != 0
        }
    };
    let is_enumerable: Option<bool> =
        with_map(descriptor, None, |m| m.get("enumerable").map(&bool_from));
    // (#749) writable flag — Object.defineProperty(obj, k, { writable: false }).
    let is_writable: Option<bool> =
        with_map(descriptor, None, |m| m.get("writable").map(&bool_from));
    // (#1073) configurable flag.
    let is_configurable: Option<bool> =
        with_map(descriptor, None, |m| m.get("configurable").map(&bool_from));
    let key_str = match str_from_abi(key_ptr, key_len) {
        Some(s) => s.to_string(),
        None => return obj,
    };
    // (#1073) Tentativa de redefinir uma prop nao-configurable joga TypeError
    // (sloppy mode tambem). Seta o error slot e retorna obj para que codigo
    // user em try/catch capture o erro com e.constructor.name === "TypeError".
    if is_non_configurable(obj, &key_str) {
        let msg = format!("Cannot redefine property: {}", key_str);
        let err_h = rts_primitives::error::instance::__RTS_FN_GL_TYPE_ERROR_NEW(
            msg.as_ptr() as i64,
            msg.len() as i64,
            0,
        );
        rts_engine::collector::error::__rtsn_error_set(err_h);
        return obj;
    }
    if matches!(is_enumerable, Some(false)) {
        mark_non_enumerable(obj, &key_str);
    } else {
        // Se redefiniu com enumerable=true (ou ausente, default keep),
        // remove marca antiga.
        rts_engine::heap::descriptors::unmark_non_enumerable(obj, &key_str);
    }
    if matches!(is_writable, Some(false)) {
        mark_non_writable(obj, &key_str);
    } else if matches!(is_writable, Some(true)) {
        unmark_non_writable(obj, &key_str);
    }
    if matches!(is_configurable, Some(false)) {
        mark_non_configurable(obj, &key_str);
    }
    // (#1111) Accessor descriptor: { get: fn } / { set: fn }. RTS armazena
    // como slots internos __get_<name>/__set_<name> (handle/fn_ptr) e
    // MAP_GET_CHAIN/MAP_SET detectam e invocam via INVOKE_AUTO.
    let getter: Option<i64> = with_map(descriptor, None, |m| {
        m.get("get").copied().filter(|v| *v != 0)
    });
    let setter: Option<i64> = with_map(descriptor, None, |m| {
        m.get("set").copied().filter(|v| *v != 0)
    });
    if getter.is_some() || setter.is_some() {
        let key_owned = key_str.clone();
        with_map_mut(obj, (), |m| {
            if let Some(g) = getter {
                m.insert(format!("__get_{}", key_owned), g);
            }
            if let Some(s) = setter {
                m.insert(format!("__set_{}", key_owned), s);
            }
            // Remove eventual data slot anterior — accessor e data sao
            // mutuamente exclusivos em JS spec.
            m.shift_remove(key_owned.as_str());
        });
        return obj;
    }
    // Bypass MAP_SET (que respeita writable=false): defineProperty sempre
    // grava o valor mesmo se ja era non-writable.
    let key_owned = key_str.clone();
    with_map_mut(obj, (), |m| {
        m.insert(key_owned, value);
    });
    obj
}
