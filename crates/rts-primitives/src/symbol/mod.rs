//! `Symbol` global class (#216) — the single opaque primitive.
//!
//! Hand-written builder registration (`register_symbol_class_spec()`) for the
//! `Symbol` global itself; `well_known.rs` is the compile-time-checked source
//! for the well-known set that `#[rtse::symbol(Symbol::iterator)]` (rts-macro)
//! points at. `__RTS_FN_RT_TO_PRIMITIVE` + the well-known externs are not
//! `Member`s of the class — they are free fns below, called by codegen by
//! symbol.

pub mod wellknown;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rts_engine::abi::member::DefaultArg;
use rts_engine::abi::ty::Handle;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

// `string_pool` (gc-surface) fica no `rts-runtime` collector; o símbolo
// `#[no_mangle]` resolve em link-time (JIT registra; AOT pelo staticlib).
unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Global registry para Symbol.for / Symbol.keyFor.
fn registry() -> &'static Mutex<HashMap<String, u64>> {
    static REG: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

// (#216) Well-known symbols: cached handles, allocated on demand.
// Each has description "Symbol.iterator", "Symbol.asyncIterator", etc,
// and is guaranteed to return the same handle on subsequent calls.
//
// The `name` list here is the RUNTIME side of the well-known set; the
// COMPILE-TIME-checked side is `wellknown::Symbol`'s associated consts (each
// `WellKnown { key }` calls back into this fn). Keep the two in lockstep —
// this is the only place a well-known name string is written.
pub(crate) fn well_known_handle(name: &str) -> u64 {
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

/// Creates a new unique Symbol with optional description string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_NEW(
    description_ptr: *const u8,
    description_len: i64,
) -> Handle {
    let description =
        unsafe { rts_engine::abi::str_abi::from_abi(description_ptr, description_len) };
    let description = description.map(|s| s.to_string());
    alloc_entry(Entry::Symbol { description })
}

/// Returns a registered symbol by key — same key always returns same handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_FOR(key_ptr: *const u8, key_len: i64) -> Handle {
    let key = match unsafe { rts_engine::abi::str_abi::from_abi(key_ptr, key_len) } {
        ::core::option::Option::Some(s) => s,
        ::core::option::Option::None => return 0,
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

/// Returns the key for a registered symbol, or 0 (undefined) if not registered.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_KEY_FOR(sym: Handle) -> Handle {
    let reg = registry().lock().unwrap();
    for (k, &h) in reg.iter() {
        if h == sym {
            let key_clone = k.clone();
            drop(reg);
            return unsafe {
                __RTS_FN_NS_GC_STRING_NEW(key_clone.as_ptr(), key_clone.len() as i64)
            };
        }
    }
    drop(reg);
    let undef = b"undefined";
    unsafe { __RTS_FN_NS_GC_STRING_NEW(undef.as_ptr(), undef.len() as i64) }
}

/// Returns the symbol's description string, or 0 if none.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_DESCRIPTION(sym: Handle) -> Handle {
    let desc = with_entry(sym, |e| match e {
        Some(Entry::Symbol { description }) => description.clone(),
        _ => None,
    });
    match desc {
        Some(s) => unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) },
        None => 0,
    }
}

/// Returns 'Symbol(description)' string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_TO_STRING(sym: Handle) -> Handle {
    let s = with_entry(sym, |e| match e {
        Some(Entry::Symbol {
            description: Some(d),
        }) => format!("Symbol({d})"),
        Some(Entry::Symbol { description: None }) => "Symbol()".to_string(),
        _ => "[invalid Symbol]".to_string(),
    });
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Symbol.iterator — well-known symbol pra iteration protocol.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_ITERATOR() -> Handle {
    well_known_handle("iterator")
}

/// Symbol.asyncIterator — async iteration protocol.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_ASYNC_ITERATOR() -> Handle {
    well_known_handle("asyncIterator")
}

/// Symbol.hasInstance — controla instanceof.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_HAS_INSTANCE() -> Handle {
    well_known_handle("hasInstance")
}

/// Symbol.toPrimitive — controla coercao.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_TO_PRIMITIVE() -> Handle {
    well_known_handle("toPrimitive")
}

/// Symbol.toStringTag — customiza Object.prototype.toString.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYMBOL_TO_STRING_TAG() -> Handle {
    well_known_handle("toStringTag")
}

/// Registra a classe global `Symbol` no motor (Fase 2 — hand-written, sem macro).
pub fn register_symbol_class_spec(e: &mut Engine) {
    e.class("Symbol")
        .doc("Built-in Symbol primitive (#216). Each Symbol() call returns a unique handle.")
        .member(Member {
            name: "new".to_string(),
            kind: MemberKind::Constructor,
            // description is OPTIONAL: `Symbol()` (0 args) and `Symbol("x")` (1) both
            // valid. The omitted StrPtr defaults to the `undefined` sentinel (the
            // runtime reads a null/empty description).
            sig: Sig::with_defaults(
                vec![AbiType::StrPtr],
                AbiType::Handle,
                vec![DefaultArg::Undefined],
            ),
            symbol: "__RTS_FN_GL_SYMBOL_NEW".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_NEW as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "new Symbol(description?: string): symbol".to_string(),
            doc: "Creates a new unique Symbol with optional description string.".to_string(),
            pure: false,
            emit: None,
        })
        .member(Member {
            name: "for".to_string(),
            kind: MemberKind::Function,
            sig: Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_FOR".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_FOR as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "for(key: string): symbol".to_string(),
            doc: "Returns a registered symbol by key — same key always returns same handle."
                .to_string(),
            pure: false,
            emit: None,
        })
        .member(Member {
            name: "keyFor".to_string(),
            kind: MemberKind::Function,
            sig: Sig::new(vec![AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_KEY_FOR".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_KEY_FOR as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "keyFor(sym: symbol): string | undefined".to_string(),
            doc: "Returns the key for a registered symbol, or 0 (undefined) if not registered."
                .to_string(),
            pure: true,
            emit: None,
        })
        .member(Member {
            name: "description".to_string(),
            kind: MemberKind::InstanceGetter,
            sig: Sig::new(vec![AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_DESCRIPTION".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_DESCRIPTION as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "description: string | undefined".to_string(),
            doc: "Returns the symbol's description string, or 0 if none.".to_string(),
            pure: true,
            emit: None,
        })
        .member(Member {
            name: "toString".to_string(),
            kind: MemberKind::InstanceMethod,
            sig: Sig::new(vec![AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_TO_STRING".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_TO_STRING as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "toString(): string".to_string(),
            doc: "Returns 'Symbol(description)' string.".to_string(),
            pure: true,
            emit: None,
        })
        .member(Member {
            name: "iterator".to_string(),
            kind: MemberKind::Constant,
            sig: Sig::new(Vec::new(), AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_ITERATOR".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_ITERATOR as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "readonly iterator: unique symbol".to_string(),
            doc: "Symbol.iterator — well-known symbol pra iteration protocol.".to_string(),
            pure: true,
            emit: None,
        })
        .member(Member {
            name: "asyncIterator".to_string(),
            kind: MemberKind::Constant,
            sig: Sig::new(Vec::new(), AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_ASYNC_ITERATOR".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_ASYNC_ITERATOR as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "readonly asyncIterator: unique symbol".to_string(),
            doc: "Symbol.asyncIterator — async iteration protocol.".to_string(),
            pure: true,
            emit: None,
        })
        .member(Member {
            name: "hasInstance".to_string(),
            kind: MemberKind::Constant,
            sig: Sig::new(Vec::new(), AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_HAS_INSTANCE".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_HAS_INSTANCE as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "readonly hasInstance: unique symbol".to_string(),
            doc: "Symbol.hasInstance — controla instanceof.".to_string(),
            pure: true,
            emit: None,
        })
        .member(Member {
            name: "toPrimitive".to_string(),
            kind: MemberKind::Constant,
            sig: Sig::new(Vec::new(), AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_TO_PRIMITIVE".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_TO_PRIMITIVE as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "readonly toPrimitive: unique symbol".to_string(),
            doc: "Symbol.toPrimitive — controla coercao.".to_string(),
            pure: true,
            emit: None,
        })
        .member(Member {
            name: "toStringTag".to_string(),
            kind: MemberKind::Constant,
            sig: Sig::new(Vec::new(), AbiType::Handle),
            symbol: "__RTS_FN_GL_SYMBOL_TO_STRING_TAG".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_SYMBOL_TO_STRING_TAG as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "readonly toStringTag: unique symbol".to_string(),
            doc: "Symbol.toStringTag — customiza Object.prototype.toString.".to_string(),
            pure: true,
            emit: None,
        })
        .done();
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

// ── `#[rtse::symbol]` demo/test class ────────────────────────────────────────
//
// Exercises both forms end-to-end through the real macro expansion (not just
// the runtime `well_known_handle` table): a well-known-symbol member and a
// registry-symbol member on the same class, asserting their Registry `Member`
// keys are distinct and correctly prefixed. `#[cfg(test)]` only — this class is
// never registered into `REGISTER`.
#[cfg(test)]
#[rtse::class("__RtseSymbolKeyDemo")]
#[derive(Clone)]
struct RtseSymbolKeyDemo;

#[cfg(test)]
#[rtse::class("__RtseSymbolKeyDemo")]
impl RtseSymbolKeyDemo {
    #[rtse::ctor]
    fn new() -> Self {
        RtseSymbolKeyDemo
    }

    /// `[Symbol.iterator]()` — well-known form, path-checked at compile time.
    #[rtse::method]
    #[rtse::symbol(wellknown::Symbol::iterator)]
    fn iter_method(self: &RtseSymbolKeyDemo) -> Handle {
        0
    }

    /// `[Symbol.for("demoKey")]()` — registry form, string-keyed.
    #[rtse::method]
    #[rtse::symbol("demoKey")]
    fn registry_method(self: &RtseSymbolKeyDemo) -> Handle {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_member_names() -> Vec<String> {
        let mut e = Engine::new();
        register(&mut e);
        e.registry()
            .class("__RtseSymbolKeyDemo")
            .expect("registered above")
            .members
            .iter()
            .map(|m| m.name.clone())
            .collect()
    }

    #[test]
    fn well_known_symbol_member_keyed_at_sign_at_sign_ident() {
        let names = demo_member_names();
        assert!(names.contains(&"@@iterator".to_string()), "{names:?}");
    }

    #[test]
    fn registry_symbol_member_keyed_sym_prefix() {
        let names = demo_member_names();
        assert!(names.contains(&"@@sym:demoKey".to_string()), "{names:?}");
    }

    #[test]
    fn well_known_and_registry_symbol_keys_are_distinct() {
        let names = demo_member_names();
        let wk = names.iter().find(|n| n.starts_with("@@") && !n.starts_with("@@sym:"));
        let reg = names.iter().find(|n| n.starts_with("@@sym:"));
        assert_ne!(wk, None);
        assert_ne!(reg, None);
        assert_ne!(wk, reg);
    }

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
