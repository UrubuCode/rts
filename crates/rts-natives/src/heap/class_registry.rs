//! (cross-runtime #47) Registry global de class hierarchy para suporte
//! a `instanceof` cross-classes (ex: `CustomTypeError extends TypeError`
//! → `err instanceof TypeError` deve retornar true).
//!
//! Codegen chama [`register_parent`] para cada class user com `extends` no
//! startup (top-level main). Os builtins (Error, TypeError, RangeError, etc)
//! sao auto-registrados em `registry()`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static REGISTRY: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
static INIT: OnceLock<()> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, String>> {
    REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();
        // Builtins: TypeError/RangeError/SyntaxError/ReferenceError
        // extends Error.
        for child in [
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "EvalError",
            "URIError",
        ] {
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

/// Registra `child extends parent` (idempotente). Usado pelo codegen p/ semear
/// os `extends` das classes `#[rtse::class]` (dado do Registry) no startup e
/// pelas classes de usuário com `extends`.
pub fn register_parent(child: &str, parent: &str) {
    ensure_init();
    if let Ok(mut m) = registry().lock() {
        m.insert(child.to_string(), parent.to_string());
    }
}

/// Retorna true se `name` eh `ancestor` ou tem `ancestor` na cadeia
/// `extends` (transitivamente).
pub fn is_descendant_of(name: &str, ancestor: &str) -> bool {
    if name == ancestor {
        return true;
    }
    ensure_init();
    let map = match registry().lock() {
        Ok(m) => m,
        Err(_) => return false,
    };
    let mut cur = name.to_string();
    // Limite contra ciclos pathologicos.
    for _ in 0..64 {
        match map.get(&cur) {
            Some(parent) => {
                if parent == ancestor {
                    return true;
                }
                cur = parent.clone();
            }
            None => return false,
        }
    }
    false
}
