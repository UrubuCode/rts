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

    // Sintetiza { value, writable: true, enumerable: true, configurable: true }
    let mut desc: IndexMap<String, i64> = IndexMap::new();
    desc.insert("value".to_string(), v);
    desc.insert("writable".to_string(), 1);
    desc.insert("enumerable".to_string(), 1);
    desc.insert("configurable".to_string(), 1);
    alloc_entry(Entry::Map(Box::new(desc)))
}

/// (cross-runtime #749) `Object.getOwnPropertyDescriptors(obj)` — Map
/// onde cada key e' uma own prop e o valor e' um descriptor sintetizado
/// {value, writable, enumerable, configurable}. Mesma sintese que
/// Reflect.getOwnPropertyDescriptor faz por key.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_DESCRIPTORS(obj: u64) -> u64 {
    let mut out: IndexMap<String, i64> = IndexMap::new();
    let pairs: Vec<(String, i64)> = with_entry(obj, |e| match e {
        Some(Entry::Map(m)) => m
            .iter()
            .filter(|(k, _)| k.as_str() != "__proto__")
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        _ => Vec::new(),
    });
    for (k, v) in pairs {
        let mut desc: IndexMap<String, i64> = IndexMap::new();
        desc.insert("value".to_string(), v);
        desc.insert("writable".to_string(), 1);
        desc.insert("enumerable".to_string(), 1);
        desc.insert("configurable".to_string(), 1);
        let desc_h = alloc_entry(Entry::Map(Box::new(desc))) as i64;
        out.insert(k, desc_h);
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
