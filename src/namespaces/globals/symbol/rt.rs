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
    let mut reg = registry().lock().unwrap();
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

/// `Symbol.keyFor(sym)` — retorna key se registrado, 0 se anonimo.
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
    0
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
    fn key_for_unregistered_returns_zero() {
        let h = __RTS_FN_GL_SYMBOL_NEW(std::ptr::null(), -1);
        assert_eq!(__RTS_FN_GL_SYMBOL_KEY_FOR(h), 0);
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
