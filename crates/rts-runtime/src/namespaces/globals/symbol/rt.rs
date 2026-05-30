//! `Symbol` runtime — extern "C" implementations.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

/// Global registry para Symbol.for / Symbol.keyFor.
fn registry() -> &'static Mutex<HashMap<String, u64>> {
    static REG: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `new Symbol(description?)` — cria handle unico.
/// `description` pode ser ptr=null/len=0 (sem description) ou string valida.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_NEW(desc_ptr: *const u8, desc_len: i64) -> u64 {
    let description = str_from_abi(desc_ptr, desc_len).map(|s| s.to_string());
    alloc_entry(Entry::Symbol { description })
}

/// `Symbol.for(key)` — retorna mesmo handle para mesma chave.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_FOR(key_ptr: *const u8, key_len: i64) -> u64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    let key_owned = key.to_string();
    let reg = registry().lock().unwrap();
    if let Some(&h) = reg.get(&key_owned) {
        return h;
    }
    drop(reg);
    let h = alloc_entry(Entry::Symbol {
        description: Some(key_owned.clone()),
    });
    let mut reg = registry().lock().unwrap();
    reg.insert(key_owned, h);
    h
}

/// `Symbol.keyFor(sym)` — retorna handle de string com a key se registrado,
/// senao retorna handle de string "undefined" (#792). JS spec: keyFor de
/// Symbol anonimo retorna `undefined`. Codegen formata handle "undefined"
/// como literal undefined em concat com template literal.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_KEY_FOR(sym: u64) -> u64 {
    let reg = registry().lock().unwrap();
    for (k, &h) in reg.iter() {
        if h == sym {
            let key_clone = k.clone();
            drop(reg);
            return crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
                key_clone.as_ptr(),
                key_clone.len() as i64,
            );
        }
    }
    drop(reg);
    let undef = b"undefined";
    crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(undef.as_ptr(), undef.len() as i64)
}

/// `sym.description` — retorna handle de string com a description, ou 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_DESCRIPTION(sym: u64) -> u64 {
    let desc = with_entry(sym, |e| match e {
        Some(Entry::Symbol { description }) => description.clone(),
        _ => None,
    });
    match desc {
        Some(s) => crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            s.as_ptr(),
            s.len() as i64,
        ),
        None => 0,
    }
}

// (#216) Well-known symbols: handles cacheados, criados sob demanda.
// Cada um tem description "Symbol.iterator", "Symbol.asyncIterator", etc,
// e e' garantido retornar mesmo handle em chamadas subsequentes.

fn well_known_handle(name: &str) -> u64 {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<&'static str, u64>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key: &'static str = match name {
        "iterator" => "iterator",
        "asyncIterator" => "asyncIterator",
        "hasInstance" => "hasInstance",
        "toPrimitive" => "toPrimitive",
        "toStringTag" => "toStringTag",
        "isConcatSpreadable" => "isConcatSpreadable",
        "match" => "match",
        "replace" => "replace",
        "search" => "search",
        "split" => "split",
        "species" => "species",
        "unscopables" => "unscopables",
        _ => return 0,
    };

    let mut guard = cache.lock().unwrap();
    if let Some(&h) = guard.get(key) {
        return h;
    }
    let desc = format!("Symbol.{key}");
    let h = alloc_entry(Entry::Symbol {
        description: Some(desc),
    });
    guard.insert(key, h);
    h
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_ITERATOR() -> u64 {
    well_known_handle("iterator")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_ASYNC_ITERATOR() -> u64 {
    well_known_handle("asyncIterator")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_HAS_INSTANCE() -> u64 {
    well_known_handle("hasInstance")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_TO_PRIMITIVE() -> u64 {
    well_known_handle("toPrimitive")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_TO_STRING_TAG() -> u64 {
    well_known_handle("toStringTag")
}

unsafe extern "C" {
    fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
}

/// (#216/274) Coercao via `[Symbol.toPrimitive](hint)`. Se `obj` for um Map
/// que tem a key `@@sym:<toPrimitive_handle>`, invoca o metodo passando o
/// hint string ("number"/"string"/"default") e devolve o resultado. Caso
/// contrario devolve `obj` inalterado (caller cai no coerce default).
///
/// hint_code: 0 = "number", 1 = "string", 2 = "default".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TO_PRIMITIVE(obj: i64, hint_code: i32) -> i64 {
    let obj_h = obj as u64;
    // So' Map pode ter [Symbol.toPrimitive]. Resolve o handle do well-known.
    let tp_sym = __RTS_FN_GL_SYMBOL_TO_PRIMITIVE();
    let key = format!("@@sym:{tp_sym}");
    let method: Option<i64> = with_entry(obj_h, |e| match e {
        Some(Entry::Map(m)) => m.get(&key).copied().filter(|v| *v != 0),
        _ => None,
    });
    let Some(method) = method else {
        return obj; // sem toPrimitive — caller usa coerce default.
    };
    let hint = match hint_code {
        0 => "number",
        1 => "string",
        _ => "default",
    };
    let hint_h = alloc_entry(Entry::String(hint.as_bytes().to_vec()));
    let args = alloc_entry(Entry::Vec(Box::new(vec![hint_h as i64])));
    // this = obj (o metodo pode usar this.<campo>).
    unsafe { __RTS_FN_RT_INVOKE_AUTO(method, obj, args) }
}

/// `sym.toString()` — "Symbol(description)" ou "Symbol()".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_TO_STRING(sym: u64) -> u64 {
    let s = with_entry(sym, |e| match e {
        Some(Entry::Symbol {
            description: Some(d),
        }) => format!("Symbol({d})"),
        Some(Entry::Symbol { description: None }) => "Symbol()".to_string(),
        _ => "[invalid Symbol]".to_string(),
    });
    crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_symbols_different_handles() {
        let a = __RTS_FN_GL_SYMBOL_NEW(std::ptr::null(), -1);
        let b = __RTS_FN_GL_SYMBOL_NEW(std::ptr::null(), -1);
        assert_ne!(a, b);
    }

    #[test]
    fn for_same_key_returns_same_handle() {
        let key = b"my_key";
        let a = __RTS_FN_GL_SYMBOL_FOR(key.as_ptr(), key.len() as i64);
        let b = __RTS_FN_GL_SYMBOL_FOR(key.as_ptr(), key.len() as i64);
        assert_eq!(a, b);
    }

    #[test]
    fn key_for_returns_registered_key() {
        let key = b"another_key";
        let h = __RTS_FN_GL_SYMBOL_FOR(key.as_ptr(), key.len() as i64);
        let result = __RTS_FN_GL_SYMBOL_KEY_FOR(h);
        assert_ne!(result, 0);
        let s = with_entry(result, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "another_key");
    }

    #[test]
    fn key_for_unregistered_returns_undefined_handle() {
        let h = __RTS_FN_GL_SYMBOL_NEW(std::ptr::null(), -1);
        let result = __RTS_FN_GL_SYMBOL_KEY_FOR(h);
        let s = with_entry(result, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "undefined");
    }

    #[test]
    fn description_returns_string() {
        let desc = b"hello";
        let h = __RTS_FN_GL_SYMBOL_NEW(desc.as_ptr(), desc.len() as i64);
        let result = __RTS_FN_GL_SYMBOL_DESCRIPTION(h);
        let s = with_entry(result, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "hello");
    }
}
