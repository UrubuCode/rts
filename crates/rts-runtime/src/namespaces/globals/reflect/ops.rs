//! Reflect ops — fns "novas" pro spec que nao mapeiam 1:1 nas Map APIs:
//!
//! - `Reflect.getOwnPropertyDescriptor(obj, key)` -> Map sintetizado
//!   `{ value, writable, enumerable, configurable }`. Descriptors reais
//!   (com get/set/writable/enumerable/configurable distintos por slot)
//!   exigiriam metadata por slot no Map — fora do escopo v0. Hoje todos
//!   os slots sao `writable: true, enumerable: true, configurable: true`.
//!
//! - `Reflect.defineProperty(obj, key, descriptor)` -> faz `obj[key] =
//!   descriptor.value` (extrai `value` do descriptor Map e faz set).
//!   Ignora `writable`/`enumerable`/`configurable` (sempre true) e
//!   accessor descriptors (`get`/`set`). Retorna 1.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};
use indexmap::IndexMap;

/// Helper: extrai o conteudo string de um handle (para usar como key
/// em map_*). Retorna None se o handle nao eh string.
fn key_bytes(handle: u64) -> Option<Vec<u8>> {
    with_entry(handle, |e| match e {
        Some(Entry::String(b)) => Some(b.clone()),
        _ => None,
    })
}

#[cfg(test)]
fn alloc_str(s: &str) -> u64 {
    alloc_entry(Entry::String(s.as_bytes().to_vec()))
}

/// `Reflect.getOwnPropertyDescriptor(obj, key)` — devolve um Map com
/// `{value, writable, enumerable, configurable}` quando a propriedade
/// existe; 0 (handle invalido) quando nao existe.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR(
    obj: u64,
    key_handle: u64,
) -> u64 {
    // (#795) JS spec: retorna undefined quando prop nao existe. Aloca
    // handle string "undefined" — TPL_COERCE_AUTO renderiza passthrough.
    let alloc_undef = || alloc_entry(Entry::String(b"undefined".to_vec()));
    let Some(key_bytes_v) = key_bytes(key_handle) else {
        return alloc_undef();
    };
    let key_str = String::from_utf8_lossy(&key_bytes_v).into_owned();

    // Eh propriedade do obj? Olha direto no IndexMap (sem chain — Reflect
    // GETOWN nao desce por __proto__).
    let value: Option<i64> = with_entry(obj, |e| match e {
        Some(Entry::Map(m)) => m.get(&key_str).copied(),
        _ => None,
    });
    let Some(v) = value else {
        return alloc_undef();
    };

    // (#795/#749) Sintetiza { value, writable, enumerable, configurable }.
    // Consulta flags reais setadas via Object.defineProperty; default true.
    let bool_true: i64 = i64::MIN + 1;
    let bool_false: i64 = i64::MIN;
    let writable = if crate::namespaces::collections::map::is_non_writable(obj, &key_str) {
        bool_false
    } else {
        bool_true
    };
    let enumerable = if crate::namespaces::collections::map::is_non_enumerable(obj, &key_str) {
        bool_false
    } else {
        bool_true
    };
    let mut desc: IndexMap<String, i64> = IndexMap::new();
    desc.insert("value".to_string(), v);
    desc.insert("writable".to_string(), writable);
    desc.insert("enumerable".to_string(), enumerable);
    desc.insert("configurable".to_string(), bool_true);
    alloc_entry(Entry::Map(Box::new(desc)))
}

/// (cross-runtime #749) `Object.getOwnPropertyDescriptors(obj)` — Map
/// onde cada key e' uma own prop e o valor e' um descriptor sintetizado.
/// Data props -> {value, writable, enumerable, configurable}.
/// Accessor props (#110 slots `__get_<name>`/`__set_<name>`) ->
/// {get, set, enumerable, configurable} — JS spec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_DESCRIPTORS(obj: u64) -> u64 {
    let pairs: Vec<(String, i64)> = with_entry(obj, |e| match e {
        Some(Entry::Map(m)) => m
            .iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    let bool_true: i64 = i64::MIN + 1;
    let bool_false: i64 = i64::MIN;
    // Particiona: data props vs accessor slots. Accessors juntam
    // get/set pelo nome base e produzem um unico descriptor.
    let mut data_pairs: Vec<(String, i64)> = Vec::new();
    let mut getters: IndexMap<String, i64> = IndexMap::new();
    let mut setters: IndexMap<String, i64> = IndexMap::new();
    for (k, v) in pairs {
        if let Some(name) = k.strip_prefix("__get_") {
            getters.insert(name.to_string(), v);
        } else if let Some(name) = k.strip_prefix("__set_") {
            setters.insert(name.to_string(), v);
        } else {
            data_pairs.push((k, v));
        }
    }
    let mut out: IndexMap<String, i64> = IndexMap::new();
    for (k, v) in data_pairs {
        // (#749) Consulta flags reais setadas via Object.defineProperty.
        let writable = if crate::namespaces::collections::map::is_non_writable(obj, &k) {
            bool_false
        } else {
            bool_true
        };
        let enumerable = if crate::namespaces::collections::map::is_non_enumerable(obj, &k) {
            bool_false
        } else {
            bool_true
        };
        let mut desc: IndexMap<String, i64> = IndexMap::new();
        desc.insert("value".to_string(), v);
        desc.insert("writable".to_string(), writable);
        desc.insert("enumerable".to_string(), enumerable);
        desc.insert("configurable".to_string(), bool_true);
        let desc_h = alloc_entry(Entry::Map(Box::new(desc))) as i64;
        out.insert(k, desc_h);
    }
    // Une chaves de getters e setters (qualquer um pode existir sozinho).
    let mut accessor_keys: Vec<String> = getters.keys().cloned().collect();
    for k in setters.keys() {
        if !accessor_keys.contains(k) {
            accessor_keys.push(k.clone());
        }
    }
    for name in accessor_keys {
        let mut desc: IndexMap<String, i64> = IndexMap::new();
        // getter/setter ausente -> undefined sentinel handle (string).
        let get_v = getters.get(&name).copied().unwrap_or_else(|| {
            alloc_entry(Entry::String(b"undefined".to_vec())) as i64
        });
        let set_v = setters.get(&name).copied().unwrap_or_else(|| {
            alloc_entry(Entry::String(b"undefined".to_vec())) as i64
        });
        desc.insert("get".to_string(), get_v);
        desc.insert("set".to_string(), set_v);
        desc.insert("enumerable".to_string(), bool_true);
        desc.insert("configurable".to_string(), bool_true);
        let desc_h = alloc_entry(Entry::Map(Box::new(desc))) as i64;
        out.insert(name, desc_h);
    }
    alloc_entry(Entry::Map(Box::new(out)))
}

/// `Reflect.defineProperty(obj, key, descriptor)` — extrai
/// `descriptor.value` e faz `obj[key] = value`. Retorna 1 (true).
/// Ignora writable/enumerable/configurable (sempre tratado como true)
/// e accessor descriptors. Sempre tem sucesso em v0 (nao ha objects
/// frozen/sealed reais).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REFLECT_DEFINE_PROPERTY(
    obj: u64,
    key_handle: u64,
    descriptor: u64,
) -> i64 {
    let Some(key_bytes_v) = key_bytes(key_handle) else {
        return 0;
    };
    let key_str = String::from_utf8_lossy(&key_bytes_v).into_owned();

    // Extrai descriptor.value. Se descriptor nao tem `value`, zera (semantica
    // proxima a JS: defineProperty com {} gera prop com value=undefined).
    let value: i64 = with_entry(descriptor, |e| match e {
        Some(Entry::Map(m)) => m.get("value").copied().unwrap_or(0),
        _ => 0,
    });

    // Escreve no obj. Reusa a logica do MAP_SET via with_entry_mut.
    use crate::namespaces::gc::handles::with_entry_mut;
    let ok = with_entry_mut(obj, |e| match e {
        Some(Entry::Map(m)) => {
            m.insert(key_str, value);
            true
        }
        _ => false,
    });
    if ok { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::gc::handles::{Entry, alloc_entry};

    #[test]
    fn descriptor_synthesizes_default_flags() {
        let mut m: IndexMap<String, i64> = IndexMap::new();
        m.insert("x".to_string(), 42);
        let obj = alloc_entry(Entry::Map(Box::new(m)));
        let key = alloc_str("x");
        let desc = __RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR(obj, key);
        assert_ne!(desc, 0);
        // Confere que tem value=42 e flags=1.
        let value = with_entry(desc, |e| match e {
            Some(Entry::Map(m)) => m.get("value").copied(),
            _ => None,
        });
        assert_eq!(value, Some(42));
        let writable = with_entry(desc, |e| match e {
            Some(Entry::Map(m)) => m.get("writable").copied(),
            _ => None,
        });
        assert_eq!(writable, Some(1));
    }

    #[test]
    fn missing_property_returns_zero() {
        let m: IndexMap<String, i64> = IndexMap::new();
        let obj = alloc_entry(Entry::Map(Box::new(m)));
        let key = alloc_str("nope");
        let desc = __RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR(obj, key);
        assert_eq!(desc, 0);
    }

    #[test]
    fn define_property_writes_value() {
        let m: IndexMap<String, i64> = IndexMap::new();
        let obj = alloc_entry(Entry::Map(Box::new(m)));
        let key = alloc_str("count");
        let mut desc: IndexMap<String, i64> = IndexMap::new();
        desc.insert("value".to_string(), 7);
        let desc_h = alloc_entry(Entry::Map(Box::new(desc)));
        let ok = __RTS_FN_GL_REFLECT_DEFINE_PROPERTY(obj, key, desc_h);
        assert_eq!(ok, 1);
        let stored = with_entry(obj, |e| match e {
            Some(Entry::Map(m)) => m.get("count").copied(),
            _ => None,
        });
        assert_eq!(stored, Some(7));
    }
}
