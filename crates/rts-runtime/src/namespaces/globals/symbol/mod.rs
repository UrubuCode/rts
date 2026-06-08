//! `Symbol` global class (#216) — primitivo unico opaco.
//!
//! Migrado ao modelo `#[rts_class]` (stage 5, `docs/specs/rts-core-engine.md`).
//! `__RTS_FN_RT_TO_PRIMITIVE` + os well-known helpers nao sao membros da classe
//! — ficam como free fns abaixo, chamados pelo codegen por simbolo.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rts_abi::ty::Handle;
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

/// Global registry para Symbol.for / Symbol.keyFor.
fn registry() -> &'static Mutex<HashMap<String, u64>> {
    static REG: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

// (#216) Well-known symbols: handles cacheados, criados sob demanda.
// Cada um tem description "Symbol.iterator", "Symbol.asyncIterator", etc,
// e e' garantido retornar mesmo handle em chamadas subsequentes.
fn well_known_handle(name: &str) -> u64 {
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

/// Built-in Symbol primitive (#216). Each Symbol() call returns a unique handle.
#[rts_class(Symbol)]
impl SymbolClass {
    /// Creates a new unique Symbol with optional description string.
    #[rts_ctor(ts = "new Symbol(description?: string): symbol", opt_str)]
    pub fn new(description: Str) -> Handle {
        let description = description.map(|s| s.to_string());
        alloc_entry(Entry::Symbol { description })
    }

    /// Returns a registered symbol by key — same key always returns same handle.
    #[rts_fn(
        name = "for",
        symbol = "__RTS_FN_GL_SYMBOL_FOR",
        ts = "for(key: string): symbol"
    )]
    pub fn sym_for(key: Str) -> Handle {
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

    /// Returns the key for a registered symbol, or 0 (undefined) if not registered.
    #[rts_fn(name = "keyFor", ts = "keyFor(sym: symbol): string | undefined", pure)]
    pub fn key_for(sym: Handle) -> Handle {
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
        crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            undef.as_ptr(),
            undef.len() as i64,
        )
    }

    /// Returns the symbol's description string, or 0 if none.
    #[rts_getter(ts = "description: string | undefined", pure)]
    pub fn description(sym: Handle) -> Handle {
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

    /// Returns 'Symbol(description)' string.
    #[rts_method(name = "toString", ts = "toString(): string", pure)]
    pub fn to_string(sym: Handle) -> Handle {
        let s = with_entry(sym, |e| match e {
            Some(Entry::Symbol {
                description: Some(d),
            }) => format!("Symbol({d})"),
            Some(Entry::Symbol { description: None }) => "Symbol()".to_string(),
            _ => "[invalid Symbol]".to_string(),
        });
        crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64)
    }

    /// Symbol.iterator — well-known symbol pra iteration protocol.
    #[rts_const(ts = "readonly iterator: unique symbol", pure)]
    pub fn iterator() -> Handle {
        well_known_handle("iterator")
    }

    /// Symbol.asyncIterator — async iteration protocol.
    #[rts_const(
        name = "asyncIterator",
        ts = "readonly asyncIterator: unique symbol",
        pure
    )]
    pub fn async_iterator() -> Handle {
        well_known_handle("asyncIterator")
    }

    /// Symbol.hasInstance — controla instanceof.
    #[rts_const(name = "hasInstance", ts = "readonly hasInstance: unique symbol", pure)]
    pub fn has_instance() -> Handle {
        well_known_handle("hasInstance")
    }

    /// Symbol.toPrimitive — controla coercao.
    #[rts_const(name = "toPrimitive", ts = "readonly toPrimitive: unique symbol", pure)]
    pub fn to_primitive() -> Handle {
        well_known_handle("toPrimitive")
    }

    /// Symbol.toStringTag — customiza Object.prototype.toString.
    #[rts_const(name = "toStringTag", ts = "readonly toStringTag: unique symbol", pure)]
    pub fn to_string_tag() -> Handle {
        well_known_handle("toStringTag")
    }
}

// ── Non-member externs (codegen calls by symbol). ────────────────────────────

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
