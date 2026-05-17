//! (cross-runtime #47) Registry global de class hierarchy para suporte
//! a `instanceof` cross-classes (ex: `CustomTypeError extends TypeError`
//! → `err instanceof TypeError` deve retornar true).
//!
//! Codegen emite chamadas a `__RTS_FN_NS_GC_CLASS_REGISTER_PARENT`
//! para cada class user com `extends` no startup (top-level main).
//! Os builtins (Error, TypeError, RangeError, etc) sao
//! auto-registrados em `init_builtins()`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static REGISTRY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
static INIT: OnceLock<()> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, String>> {
    REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();
        // Builtins: TypeError/RangeError/SyntaxError/ReferenceError
        // extends Error.
        for child in ["TypeError", "RangeError", "SyntaxError", "ReferenceError", "EvalError", "URIError"] {
            m.insert(child.to_string(), "Error".to_string());
        }
        Mutex::new(m)
    })
}

fn ensure_init() {
    INIT.get_or_init(|| {
        let _ = registry();
    });
}

/// Registra `child extends parent`. Idempotente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_GC_CLASS_REGISTER_PARENT(
    child_ptr: i64,
    child_len: i64,
    parent_ptr: i64,
    parent_len: i64,
) {
    if child_ptr == 0 || child_len <= 0 || parent_ptr == 0 || parent_len <= 0 {
        return;
    }
    ensure_init();
    let child = unsafe {
        std::slice::from_raw_parts(child_ptr as *const u8, child_len as usize)
    };
    let parent = unsafe {
        std::slice::from_raw_parts(parent_ptr as *const u8, parent_len as usize)
    };
    let child_s = match std::str::from_utf8(child) { Ok(s) => s, Err(_) => return };
    let parent_s = match std::str::from_utf8(parent) { Ok(s) => s, Err(_) => return };
    if let Ok(mut m) = registry().lock() {
        m.insert(child_s.to_string(), parent_s.to_string());
    }
}

/// Retorna true se `name` eh `ancestor` ou tem `ancestor` na cadeia
/// `extends` (transitivamente).
pub fn is_descendant_of(name: &str, ancestor: &str) -> bool {
    if name == ancestor { return true; }
    ensure_init();
    let map = match registry().lock() { Ok(m) => m, Err(_) => return false };
    let mut cur = name.to_string();
    // Limite contra ciclos pathologicos.
    for _ in 0..64 {
        match map.get(&cur) {
            Some(parent) => {
                if parent == ancestor { return true; }
                cur = parent.clone();
            }
            None => return false,
        }
    }
    false
}
