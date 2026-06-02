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

/// (#798) Keys com prefixo `@@sym:` sao a repr canonica de Symbol entries
/// (escrita pelo codegen via OBJ_SET/MAP_SET_KH). NAO sao expostas em
/// Object.keys/getOwnPropertyNames/values/entries — JS spec separa
/// string-keyed (Object.keys) de symbol-keyed (getOwnPropertySymbols).
fn is_symbol_key(k: &str) -> bool {
    k.starts_with("@@sym:")
}

/// (#798) Decodifica `@@sym:<handle>` -> handle do Entry::Symbol.
fn decode_symbol_key(k: &str) -> Option<u64> {
    k.strip_prefix("@@sym:").and_then(|s| s.parse::<u64>().ok())
}

/// (#216/299) True se a key `@@sym:<h>` referencia o well-known
/// `Symbol.iterator` (handle sticky cacheado em globals/symbol/rt).
fn key_is_well_known_iterator(k: &str) -> bool {
    match decode_symbol_key(k) {
        Some(h) => h == crate::namespaces::globals::symbol::rt::__RTS_FN_GL_SYMBOL_ITERATOR(),
        None => false,
    }
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

pub(crate) fn with_map_mut<F, R>(handle: u64, default: R, f: F) -> R
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

/// (cross-runtime #1125) Storage do `globalThis` para escrita/leitura
/// dinamica (`globalThis[k] = v` / `globalThis[k]` / `k in globalThis`).
/// Singleton lazy-init via OnceLock — handle Map persistente durante
/// toda a sessao.
static GLOBAL_THIS_HANDLE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_GLOBAL_THIS_MAP() -> u64 {
    *GLOBAL_THIS_HANDLE.get_or_init(|| {
        alloc_entry(Entry::Map(Box::new(IndexMap::new())))
    })
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
    // (cross-runtime #1071) Walk __proto__ chain sem disparar Proxy trap.
    // Usado em lookup de slots sentinela `__get_<k>` / `__set_<k>` que
    // podem viver no prototype Map de uma classe (caso classico:
    // `Object.defineProperty(Widget.prototype, "visible", {get, set})`).
    // Como o lookup eh para slots internos do RTS (nao chaves user-visible),
    // walk recursivo eh seguro.
    let mut current = handle;
    let mut depth = 0u32;
    while current != 0 && depth < 64 {
        let found = with_map(current, 0i64, |m| m.get(key).copied().unwrap_or(0));
        if found != 0 {
            return found;
        }
        let next = with_map(current, 0i64, |m| m.get("__proto__").copied().unwrap_or(0));
        if next == 0 {
            return 0;
        }
        current = next as u64;
        depth += 1;
    }
    0
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
    // (#1080-format) Object.create(null) cria objeto sem prototype.
    // Marca para preservar a semantica mesmo se user setar __proto__
    // depois (em null-proto objects, __proto__ vira regular property).
    if proto == 0 {
        null_proto_set().lock().unwrap().insert(h);
    }
    h
}

fn null_proto_set() -> &'static std::sync::Mutex<std::collections::HashSet<u64>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<u64>>> = std::sync::OnceLock::new();
    S.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

pub fn is_null_proto_handle(handle: u64) -> bool {
    null_proto_set().lock().unwrap().contains(&handle)
}

/// (#162) `Object.create(proto, descriptors)` 2-arg variant — aplica
/// descriptors ao objeto. Cada entry de `descs` deve ser
/// `{ key: { value, writable?, enumerable?, configurable? } }`.
/// Para v0, extraimos so o `value` e fazemos MAP_SET. Tambem respeita
/// `enumerable: false` via mark_non_enumerable. writable/configurable
/// nao sao enforced ainda.
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
        // Extrai value + enumerable. Sentinel JS para false eh i64::MIN
        // (TPL coercion convention). enumerable defaulta a true quando
        // ausente em Object.defineProperty mas a false em Object.create
        // descriptors — aqui assumimos defineProperty-like (default true).
        let (value, is_non_enum): (i64, bool) = with_entry(desc_h, |e| match e {
            Some(Entry::Map(d)) => {
                let v = d.get("value").copied().unwrap_or(0);
                let enum_v = d.get("enumerable").copied();
                let non_enum = matches!(enum_v, Some(x) if x == i64::MIN);
                (v, non_enum)
            }
            _ => (desc_h_i, false),
        });
        with_map_mut(target, (), |m| {
            m.insert(key.clone(), value);
        });
        if is_non_enum {
            mark_non_enumerable(target, &key);
        }
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

/// (cross-runtime #798) `Object.getOwnPropertySymbols(obj)` — retorna
/// Vec<i64> com handles de Symbol entries (keys `@@sym:<handle>`).
/// Ordem: insercao no IndexMap.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_SYMBOLS(handle: u64) -> u64 {
    let syms: Vec<i64> = with_map(handle, Vec::new(), |m| {
        m.keys()
            .filter_map(|k| decode_symbol_key(k).map(|h| h as i64))
            .collect()
    });
    alloc_entry(Entry::Vec(Box::new(syms)))
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
    // (cross-runtime #340) Proxy: dispatch `has` trap quando obj_h eh Proxy.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(obj_h) {
        return crate::namespaces::globals::proxy::ops::dispatch_has(target, handler, &key);
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
    // Para instances de classe, methods sao fns globais (nao slots em
    // instance), entao consultamos class registry via __rts_class tag.
    if with_map(obj_h, 0, |m| if m.contains_key(&key) { 1 } else { 0 }) == 1 {
        return 1;
    }
    // Walk __proto__ chain.
    let mut visited: HashSet<u64> = HashSet::new();
    let mut cur = with_map(obj_h, 0i64, |m| m.get("__proto__").copied().unwrap_or(0)) as u64;
    let mut steps = 0u32;
    while cur != 0 && steps < 64 {
        steps += 1;
        if !visited.insert(cur) { break; }
        let found = with_map(cur, 0, |m| if m.contains_key(&key) { 1 } else { 0 });
        if found == 1 { return 1; }
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
        // (#1114) Class method registry: emit no init de __RTS_MAIN
        // registra cada method declarado por classe; consulta por
        // __rts_class tag da instancia.
        if class_method_registry()
            .lock()
            .unwrap()
            .get(cls)
            .map(|set| set.contains(&key))
            .unwrap_or(false)
        {
            return 1;
        }
        // Object.prototype tem toString, hasOwnProperty, valueOf, isPrototypeOf,
        // propertyIsEnumerable, toLocaleString.
        if matches!(
            key.as_str(),
            "toString" | "hasOwnProperty" | "valueOf" | "isPrototypeOf"
            | "propertyIsEnumerable" | "toLocaleString" | "constructor"
        ) {
            return 1;
        }
    }
    0
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

/// (#1114) Registry de methods declarados por classe. Populado pelo
/// codegen via __RTS_FN_NS_COLLECTIONS_REGISTER_CLASS_METHOD no inicio
/// de __RTS_MAIN; consultado pelo `in` operator (OBJ_HAS).
fn class_method_registry() -> &'static std::sync::Mutex<
    std::collections::HashMap<String, std::collections::HashSet<String>>,
> {
    static R: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, std::collections::HashSet<String>>>,
    > = std::sync::OnceLock::new();
    R.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_REGISTER_CLASS_METHOD(
    class_ptr: *const u8,
    class_len: i64,
    method_ptr: *const u8,
    method_len: i64,
) {
    let Some(cls) = str_from_abi(class_ptr, class_len) else { return };
    let Some(method) = str_from_abi(method_ptr, method_len) else { return };
    class_method_registry()
        .lock()
        .unwrap()
        .entry(cls.to_string())
        .or_insert_with(std::collections::HashSet::new)
        .insert(method.to_string());
}

/// (cross-runtime #753) `MAP_GET` aceitando key como handle. Retorna
/// 0 se ausente (use OBJ_HAS pra distinguir).
/// (#94) Receiver Vec: tenta parse de key como int e usa VEC_GET. Sem
/// isso, `arr[k as any]` (k = "0", "1", ...) sempre retornava 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GET_KH(obj_h: u64, key_h: u64) -> i64 {
    let Some(key) = key_handle_to_string(key_h) else {
        return 0;
    };
    // (cross-runtime #340/#53) Proxy: dispatch `get` trap antes do
    // path raw — consistente com MAP_HAS/MAP_GET/MAP_SET. Sem isso,
    // `Reflect.get(proxy, key)` em fn body via codegen MAP_GET_KH
    // pulava o trap.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(obj_h) {
        return crate::namespaces::globals::proxy::ops::dispatch_get(target, handler, &key);
    }
    // Vec path: parse key como array index.
    let is_vec = with_entry(obj_h, |e| matches!(e, Some(Entry::Vec(_))));
    if is_vec {
        if let Some(n) = parse_array_index(&key) {
            return super::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(obj_h, n as i64);
        }
        // (#216/299) `arr[Symbol.iterator]` -> handle Function nativo
        // (ARRAY_VALUES_ITER). typeof === "function", chamavel com this=arr.
        if key_is_well_known_iterator(&key) {
            return crate::namespaces::gc::generator::__RTS_FN_GL_ARRAY_ITERATOR_FN() as i64;
        }
        return 0;
    }
    // Map: valor armazenado vence (ex: custom [Symbol.iterator]); se ausente
    // e a key for o iterator well-known, devolve o iterator nativo de array
    // (Map iteravel via entries — fallback minimo).
    let stored = with_map(obj_h, 0, |m| m.get(&key).copied().unwrap_or(0));
    if stored == 0 && key_is_well_known_iterator(&key) {
        return crate::namespaces::gc::generator::__RTS_FN_GL_ARRAY_ITERATOR_FN() as i64;
    }
    stored
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
    // Para Set, usa INVOKE_AUTO (suporta user fns com calling conv tail).
    // Para Map, mantem transmute fn(i64,i64,i64) — callbacks de Map sao
    // normalmente arrows lifted (windows_fastcall) que aceitam essa sig.
    if is_set {
        for (k, v) in &pairs {
            // (#394) elemento via value preservado (identidade), nao parse da key.
            let key_for_set: i64 = set_element_from_pair(k, *v);
            let args_vec = alloc_entry(Entry::Vec(Box::new(vec![key_for_set, key_for_set, handle as i64])));
            crate::namespaces::globals::function::ops::__RTS_FN_RT_INVOKE_AUTO(
                fn_ptr as i64,
                0,
                args_vec,
            );
        }
        return;
    }
    for (k, v) in pairs {
        let key_h = alloc_entry(Entry::String(k.into_bytes())) as i64;
        unsafe {
            use std::mem::transmute;
            match n_bound {
                0 => transmute::<u64, extern "C" fn(i64, i64, i64) -> i64>(actual_ptr)(
                    v, key_h, handle as i64,
                ),
                1 => transmute::<u64, extern "C" fn(i64, i64, i64, i64) -> i64>(actual_ptr)(
                    bound[0], v, key_h, handle as i64,
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
    let fn_field: Option<i64> = crate::namespaces::gc::handles::with_entry(handle, |e| match e {
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
    // (#1073) writable=false definido via Object.defineProperty bloqueia
    // assignment silenciosamente (sloppy mode). Strict mode joga TypeError —
    // RTS opera em sloppy por default (sem 'use strict').
    if is_non_writable(handle, key) {
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
    // (#394) Set.keys() eh alias de Set.values() em JS — retorna os ELEMENTOS
    // (com identidade preservada), nao as keys-string internas.
    if handle_is_set_kind(handle) {
        return __RTS_FN_NS_COLLECTIONS_MAP_VALUES(handle);
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
            // (#798) Symbol keys: separadas via getOwnPropertySymbols.
            if is_symbol_key(k.as_str()) {
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
///
/// (#316) Para Set (storage interno Map<key_str, 1>), os "values"
/// sao na verdade as keys (elementos do Set). Detecta via
/// handle_is_set_kind e retorna parse_array_index(key) ou string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_VALUES(handle: u64) -> u64 {
    if handle_is_set_kind(handle) {
        // Set.values() retorna os elementos. (#394) Recupera a IDENTIDADE do
        // elemento a partir do value preservado (set_element_from_pair); cai
        // no parse da key apenas no fallback legado/float.
        let elems: Vec<i64> = with_map(handle, Vec::new(), |m| {
            m.iter()
                .filter(|(k, _)| k.as_str() != "__proto__")
                .map(|(k, &v)| set_element_from_pair(k, v))
                .collect()
        });
        return crate::namespaces::gc::handles::alloc_entry(
            crate::namespaces::gc::handles::Entry::Vec(Box::new(elems)),
        );
    }
    let vals: Vec<i64> = with_map(handle, Vec::new(), |m| {
        let mut int_entries: Vec<(u32, i64)> = Vec::new();
        let mut str_entries: Vec<i64> = Vec::new();
        for (k, v) in m.iter() {
            if k.as_str() == "__proto__" { continue; }
            // (#798) Symbol keys nao aparecem em Object.keys/values/entries/
            // getOwnPropertyNames — JS spec.
            if is_symbol_key(k.as_str()) { continue; }
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
            // (#798) Symbol keys nao aparecem em Object.keys/values/entries/
            // getOwnPropertyNames — JS spec.
            if is_symbol_key(k.as_str()) { continue; }
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
    // (#394) Set.entries() em JS retorna pares [elem, elem] (cada elemento
    // duplicado). Usa o elemento com identidade preservada, nao a key-string.
    if handle_is_set_kind(handle) {
        let elems: Vec<i64> = with_map(handle, Vec::new(), |m| {
            m.iter()
                .filter(|(k, _)| k.as_str() != "__proto__")
                .map(|(k, &v)| set_element_from_pair(k, v))
                .collect()
        });
        let mut outer: Vec<i64> = Vec::with_capacity(elems.len());
        for e in elems {
            let inner = crate::namespaces::gc::handles::alloc_entry(
                crate::namespaces::gc::handles::Entry::Vec(Box::new(vec![e, e])),
            );
            outer.push(inner as i64);
        }
        return crate::namespaces::gc::handles::alloc_entry(
            crate::namespaces::gc::handles::Entry::Vec(Box::new(outer)),
        );
    }
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
    // (cross-runtime #244) Unwrap StringBox antes da despachada principal.
    let unwrap_h = with_entry(handle, |e| match e {
        Some(Entry::StringBox(h)) => *h,
        _ => handle,
    });
    let handle = unwrap_h;
    let result: Vec<i64> = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => {
            let mut int_keys: Vec<(u32, String)> = Vec::new();
            let mut str_keys: Vec<String> = Vec::new();
            for k in m.keys() {
                if k.as_str() == "__proto__" { continue; }
                // (#798) Symbol keys: separadas via getOwnPropertySymbols.
                if is_symbol_key(k.as_str()) { continue; }
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
        Some(Entry::String(b)) => {
            // (#789) `new String("hi")` -> getOwnPropertyNames retorna
            // ["0","1",...,"length"] usando UTF-16 code units (mesmo
            // criterio de String.length em JS).
            let char_count = String::from_utf8_lossy(b).encode_utf16().count();
            let mut out: Vec<i64> = (0..char_count)
                .map(|i| alloc_entry(Entry::String(i.to_string().into_bytes())) as i64)
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
    // (#218 phase2 / #98) Proxy: trap `ownKeys` + filtragem por enumeravel
    // via trap `getOwnPropertyDescriptor` por chave (ECMA-262
    // OrdinaryOwnPropertyKeys). Reflect.ownKeys usa dispatch_own_keys cru.
    if let Some((target, handler)) = crate::namespaces::globals::proxy::ops::resolve_proxy(handle) {
        return crate::namespaces::globals::proxy::ops::dispatch_own_keys_enumerable(target, handler);
    }
    // (#253) StringBox unwrap antes do dispatch.
    let unwrap: Option<u64> = with_entry(handle, |e| match e {
        Some(Entry::StringBox(h)) => Some(*h),
        _ => None,
    });
    let handle = unwrap.unwrap_or(handle);
    // (#1097) Map JS (new Map(...)) e Set JS nao expoem entradas via
    // Object.keys / for-in — sao iteraveis por .entries()/.keys()/.values()
    // ou for-of. JS spec: for-in em Map vazio retorna [].
    if handle_is_map_kind(handle) || handle_is_set_kind(handle) {
        return alloc_entry(Entry::Vec(Box::new(Vec::new())));
    }
    let result: Vec<i64> = with_entry(handle, |e| match e {
        Some(Entry::Map(m)) => {
            // Ordem JS (ECMA-262 OrdinaryOwnPropertyKeys): integer-indexed
            // ascendentes, depois string keys em ordem de insercao.
            let mut int_keys: Vec<(u32, String)> = Vec::new();
            let mut str_keys: Vec<String> = Vec::new();
            for k in m.keys() {
                if k.as_str() == "__proto__" { continue; }
                // (#1097) __rts_class eh tag interno usado pelo dispatch
                // virtual de metodos de classe; nao aparece em for-in JS.
                if k.as_str() == "__rts_class" { continue; }
                // (#798) Symbol keys: separadas via getOwnPropertySymbols.
                if is_symbol_key(k.as_str()) { continue; }
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

/// (#1097) for-in JS enumera proprias + herdadas enumeraveis ao longo da
/// prototype chain (via slot __proto__ setado por Object.create / class).
/// Wraper sobre OBJECT_KEYS_AUTO que anexa keys do(s) ancestor(s) que ainda
/// nao apareceram, preservando ordem JS (own first, depois chain).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_FOR_IN_KEYS(handle: u64) -> u64 {
    // Map/Set JS: nenhuma key visivel em for-in (Map/Set tem .entries()).
    if handle_is_map_kind(handle) || handle_is_set_kind(handle) {
        return alloc_entry(Entry::Vec(Box::new(Vec::new())));
    }
    let base = __RTS_FN_NS_COLLECTIONS_OBJECT_KEYS_AUTO(handle);
    let mut keys_str: Vec<String> = with_entry(base, |e| match e {
        Some(Entry::Vec(v)) => v
            .iter()
            .map(|h| {
                with_entry(*h as u64, |se| match se {
                    Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                    _ => String::new(),
                })
            })
            .collect(),
        _ => Vec::new(),
    });
    let mut seen: HashSet<String> = keys_str.iter().cloned().collect();
    // Walk __proto__ chain. Limita profundidade para guardar contra ciclos.
    let mut cur_proto: u64 = with_map(handle, 0i64, |m| {
        m.get("__proto__").copied().unwrap_or(0)
    }) as u64;
    let mut visited: HashSet<u64> = HashSet::new();
    let mut steps = 0u32;
    while cur_proto != 0 && steps < 64 {
        steps += 1;
        if !visited.insert(cur_proto) { break; }
        // Se chain ancestral eh Map/Set JS, parar.
        if handle_is_map_kind(cur_proto) || handle_is_set_kind(cur_proto) { break; }
        let proto_keys: Vec<String> = with_entry(cur_proto, |e| match e {
            Some(Entry::Map(m)) => m
                .keys()
                .filter(|k| {
                    k.as_str() != "__proto__"
                        && k.as_str() != "__rts_class"
                        && !is_symbol_key(k)
                        && !k.starts_with("__get_")
                        && !k.starts_with("__set_")
                })
                .filter(|k| !is_non_enumerable(cur_proto, k))
                .cloned()
                .collect(),
            _ => Vec::new(),
        });
        for k in proto_keys {
            if seen.insert(k.clone()) {
                keys_str.push(k);
            }
        }
        let next = with_map(cur_proto, 0i64, |m| m.get("__proto__").copied().unwrap_or(0));
        cur_proto = next as u64;
    }
    let out: Vec<i64> = keys_str
        .into_iter()
        .map(|k| alloc_entry(Entry::String(k.into_bytes())) as i64)
        .collect();
    alloc_entry(Entry::Vec(Box::new(out)))
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

/// (#394) Recupera o ELEMENTO de um Set a partir do par (key, value) do Map
/// interno. Os escritores (Set.add, new Set([...]), SET_FROM_VEC) gravam o
/// VALOR original do elemento como value, preservando a identidade (handle de
/// objeto/Set aninhado/string). A leitura prefere esse value.
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
            Err(_) => crate::namespaces::gc::handles::alloc_entry(
                crate::namespaces::gc::handles::Entry::String(key.as_bytes().to_vec()),
            ) as i64,
        }
    } else {
        value
    }
}

/// (#316) Helper interno para estruturas que clonam (structuredClone)
/// preservarem kind=Set do source no novo handle.
pub(crate) fn mark_set_kind(handle: u64) {
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
    // (#1080-format) Object.create(null) handles tem proto = null mesmo
    // que .__proto__ slot tenha sido sobrescrito pelo user (regular prop).
    if is_null_proto_handle(handle) {
        return 0;
    }
    // (cross-runtime #336) Topo da prototype chain: o singleton
    // Object.prototype (Map real instalado como __proto__ raiz das classes)
    // nao tem proto — getPrototypeOf retorna null. Sem isso, iterar a chain
    // (`while (proto) { ...; proto = getPrototypeOf(proto); }`) dava uma
    // volta extra retornando o sentinel `[Object.prototype]` em vez de
    // terminar, imprimindo lixo (` 0`) apos "Object".
    if handle == crate::namespaces::globals::function::ops::__RTS_FN_RT_OBJECT_PROTOTYPE_HANDLE() {
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

/// (#1073) Tracking de configurable=false. Default JS para
/// Object.defineProperty (sem flag) eh false; mas RTS so' marca
/// quando explicitamente false para compat com object literal
/// (campo de literal eh configurable=true).
fn non_configurable_set() -> &'static Mutex<HashSet<(u64, String)>> {
    static S: OnceLock<Mutex<HashSet<(u64, String)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(crate) fn is_non_configurable(handle: u64, key: &str) -> bool {
    non_configurable_set()
        .lock()
        .unwrap()
        .contains(&(handle, key.to_string()))
}

pub(crate) fn mark_non_configurable(handle: u64, key: &str) {
    non_configurable_set()
        .lock()
        .unwrap()
        .insert((handle, key.to_string()));
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
    let bool_from = |v: &i64| -> bool {
        if *v == i64::MIN { false }
        else if *v == i64::MIN + 1 { true }
        else { *v != 0 }
    };
    let is_enumerable: Option<bool> = with_map(descriptor, None, |m| {
        m.get("enumerable").map(&bool_from)
    });
    // (#749) writable flag — Object.defineProperty(obj, k, { writable: false }).
    let is_writable: Option<bool> = with_map(descriptor, None, |m| {
        m.get("writable").map(&bool_from)
    });
    // (#1073) configurable flag.
    let is_configurable: Option<bool> = with_map(descriptor, None, |m| {
        m.get("configurable").map(&bool_from)
    });
    let key_str = match str_from_abi(key_ptr, key_len) {
        Some(s) => s.to_string(),
        None => return obj,
    };
    // (#1073) Tentativa de redefinir uma prop nao-configurable joga TypeError
    // (sloppy mode tambem). Seta o error slot e retorna obj para que codigo
    // user em try/catch capture o erro com e.constructor.name === "TypeError".
    if is_non_configurable(obj, &key_str) {
        let msg = format!("Cannot redefine property: {}", key_str);
        let err_h = crate::namespaces::globals::error::instance::__RTS_FN_GL_TYPE_ERROR_NEW(
            msg.as_ptr() as i64,
            msg.len() as i64,
            0,
        );
        crate::namespaces::gc::error::__RTS_FN_RT_ERROR_SET(err_h);
        return obj;
    }
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
    let getter: Option<i64> = with_map(descriptor, None, |m| m.get("get").copied().filter(|v| *v != 0));
    let setter: Option<i64> = with_map(descriptor, None, |m| m.get("set").copied().filter(|v| *v != 0));
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
        // Extrai (key_handle, value) SEM aninhar with_entry (evita reentrar
        // no lock do mesmo shard -> deadlock). Resolve a String da chave
        // numa segunda passada, ja' fora do lock do par.
        let pair_kv: Option<(i64, i64)> = with_entry(pair_h, |entry| match entry {
            Some(Entry::Vec(pair)) if pair.len() >= 2 => Some((pair[0], pair[1])),
            _ => None,
        });
        let Some((key_raw, val)) = pair_kv else { continue };
        let key_h = key_raw as u64;
        let key_str: Option<String> = with_entry(key_h, |ke| match ke {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        // (374) Chave numerica (`new Map([[1,"one"]])`): key_raw eh um i64
        // cru, nao handle de String. Usa a representacao decimal como key,
        // consistente com string_from_i64 do caminho de array literal.
        let key = key_str.unwrap_or_else(|| key_raw.to_string());
        with_map_mut(m, (), |mm| {
            mm.insert(key, val);
        });
    }
    m
}

/// (cross-runtime) `new Set(vec)` onde vec NAO eh literal (vem por param/var).
/// Cria um Map marcado como Set e adiciona cada elemento como key (value=1).
/// Espelha a conversao do caminho de array literal: elemento handle-string usa
/// o conteudo; inteiro usa a representacao decimal. Dedup natural (HashMap).
/// (#394) Deriva a KEY ESTAVEL de um elemento de Set a partir do seu valor i64
/// cru. A key serve para dedup (has/add/delete); o VALUE armazenado preserva a
/// identidade. Regras:
/// - `Entry::String` -> conteudo da string (dedup por valor: `"a"` == `"a"`).
/// - outro Entry (objeto/Map/Set/Vec/etc) -> `"\0obj#<handle>"`: dedup por
///   IDENTIDADE (handle unico). Antes objetos viravam key vazia (STRING_PTR de
///   nao-string = ptr nulo) e colidiam todos numa unica entrada.
/// - sem Entry valido (numero/bool/sentinel) -> representacao decimal.
pub(crate) fn set_stable_key(elem_raw: i64) -> String {
    use crate::namespaces::gc::handles::{Entry, with_entry};
    with_entry(elem_raw as u64, |e| match e {
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        Some(_) => format!("\0obj#{elem_raw}"),
        None => elem_raw.to_string(),
    })
}

/// (#394) `set.add(elem)` — insere preservando identidade. A KEY vem de
/// `set_stable_key` (dedup correto p/ objetos), o VALUE eh o elemento cru
/// (leitura recupera a identidade via `set_element_from_pair`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_ADD(set_h: u64, elem_raw: i64) -> u64 {
    if set_h == 0 || elem_raw == i64::MIN + 4 {
        return set_h;
    }
    let key = set_stable_key(elem_raw);
    let sealed = is_map_sealed(set_h);
    with_map_mut(set_h, (), |m| {
        if sealed && !m.contains_key(&key) {
            return;
        }
        m.insert(key, elem_raw);
    });
    set_h
}

/// (#394) `coll.has(arg)` unificado Map/Set. Para Set usa a key estavel
/// derivada do elemento cru (`elem_raw`) — identidade p/ objetos. Para Map (e
/// qualquer handle nao-Set) usa a key-string (`key_ptr`/`key_len`), preservando
/// 100% o comportamento de `MAP_HAS`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_OR_MAP_HAS(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
    elem_raw: i64,
) -> i64 {
    if handle_is_set_kind(handle) {
        let key = set_stable_key(elem_raw);
        return with_map(handle, false, |m| m.contains_key(&key)) as i64;
    }
    __RTS_FN_NS_COLLECTIONS_MAP_HAS(handle, key_ptr, key_len)
}

/// (#394) `coll.delete(arg)` unificado Map/Set. Mesma logica de
/// `SET_OR_MAP_HAS`: Set por identidade (elem_raw), Map por key-string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_OR_MAP_DELETE(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
    elem_raw: i64,
) -> i64 {
    if handle_is_set_kind(handle) {
        if is_map_frozen(handle) {
            return 0;
        }
        let key = set_stable_key(elem_raw);
        return with_map_mut(handle, false, |m| m.shift_remove(&key).is_some()) as i64;
    }
    __RTS_FN_NS_COLLECTIONS_MAP_DELETE(handle, key_ptr, key_len)
}

/// (#394) Normaliza um iteravel de for-of quando o codegen NAO conhece a
/// classe estatica (ex: o bind eh elemento de outro for-of, como em
/// `for (const s of setOfSets) for (const n of s)`). Decide em runtime:
/// - Set  -> Vec dos ELEMENTOS (MAP_VALUES, identidade preservada);
/// - Map  -> Vec de pares [k,v] (ENTRIES_INSERTION);
/// - resto (Vec/Buffer/string/etc) -> o proprio handle inalterado.
/// So' eh chamado no ramo "classe desconhecida"; o caminho tipado comum
/// (Vec anotado) nao passa por aqui.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_FOR_OF_NORMALIZE(handle: u64) -> u64 {
    if handle_is_set_kind(handle) {
        return __RTS_FN_NS_COLLECTIONS_MAP_VALUES(handle);
    }
    if handle_is_map_kind(handle) {
        return __RTS_FN_NS_COLLECTIONS_MAP_ENTRIES_INSERTION(handle);
    }
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_FROM_VEC(src: u64) -> u64 {
    use crate::namespaces::gc::handles::{Entry, with_entry};
    let elems: Vec<i64> = with_entry(src, |entry| match entry {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let m = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    __RTS_FN_NS_COLLECTIONS_MARK_AS_SET(m);
    for raw in elems {
        // hole sentinel: ignora.
        if raw == i64::MIN + 4 { continue; }
        // (#394) key estavel (identidade p/ objetos) + value = elemento cru.
        let key = set_stable_key(raw);
        with_map_mut(m, (), |mm| {
            mm.insert(key, raw);
        });
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
