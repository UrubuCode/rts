//! `Boolean` runtime — extern "C" implementations.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

/// Sentinel para `undefined` em RTS (NaN-tagged i64). Mantido em sync com
/// outras coercoes do codegen: 0 = false, qualquer outro valor = true,
/// excepto sentinel undefined.
const UNDEFINED_SENTINEL: i64 = i64::MIN;

/// `Boolean(x)` — coerce truthy/falsy. Implementacao pura em runtime para
/// quando o codegen nao consegue resolver inline (fallback).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_COERCE(value: i64) -> i64 {
    if value == 0 || value == UNDEFINED_SENTINEL {
        0
    } else {
        1
    }
}

/// Quando `recv` for handle de `Entry::BooleanBox`, retorna o bool boxed.
/// Senao, fallback para interpretar `recv` como i64 primitivo (`recv != 0`).
fn unbox_bool(recv: i64) -> bool {
    if recv > 0 {
        let h = recv as u64;
        let boxed = with_entry(h, |e| match e {
            Some(Entry::BooleanBox(b)) => Some(*b),
            _ => None,
        });
        if let Some(b) = boxed {
            return b;
        }
    }
    recv != 0
}

/// `new Boolean(x)` — aloca `Entry::BooleanBox` e retorna handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_NEW(value: i64) -> u64 {
    let b = value != 0 && value != UNDEFINED_SENTINEL;
    alloc_entry(Entry::BooleanBox(b))
}

/// `new Boolean()` — sem args, equivalente a `new Boolean(false)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_NEW_EMPTY() -> u64 {
    alloc_entry(Entry::BooleanBox(false))
}

/// `b.toString()` — handle de string "true" ou "false". Aceita tanto handle
/// boxed (`Entry::BooleanBox`) quanto i64 primitivo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_TO_STRING(b: i64) -> u64 {
    let s: &[u8] = if unbox_bool(b) { b"true" } else { b"false" };
    crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64)
}

/// `b.valueOf()` — retorna o proprio bool (i8 0/1, AbiType::Bool).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_VALUE_OF(b: i64) -> i64 {
    if unbox_bool(b) { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::gc::handles::{Entry, with_entry};

    #[test]
    fn coerce_zero_false() {
        assert_eq!(__RTS_FN_GL_BOOLEAN_COERCE(0), 0);
    }

    #[test]
    fn coerce_nonzero_true() {
        assert_eq!(__RTS_FN_GL_BOOLEAN_COERCE(1), 1);
        assert_eq!(__RTS_FN_GL_BOOLEAN_COERCE(42), 1);
        assert_eq!(__RTS_FN_GL_BOOLEAN_COERCE(-1), 1);
    }

    #[test]
    fn coerce_undefined_sentinel_false() {
        assert_eq!(__RTS_FN_GL_BOOLEAN_COERCE(UNDEFINED_SENTINEL), 0);
    }

    #[test]
    fn to_string_true() {
        let h = __RTS_FN_GL_BOOLEAN_TO_STRING(1);
        let s = with_entry(h, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "true");
    }

    #[test]
    fn to_string_false() {
        let h = __RTS_FN_GL_BOOLEAN_TO_STRING(0);
        let s = with_entry(h, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "false");
    }

    #[test]
    fn value_of_normalizes() {
        assert_eq!(__RTS_FN_GL_BOOLEAN_VALUE_OF(0), 0);
        assert_eq!(__RTS_FN_GL_BOOLEAN_VALUE_OF(1), 1);
        assert_eq!(__RTS_FN_GL_BOOLEAN_VALUE_OF(42), 1);
    }

    #[test]
    fn new_boxed_typeof_object() {
        let h = __RTS_FN_GL_BOOLEAN_NEW(1);
        let b = with_entry(h, |e| match e {
            Some(Entry::BooleanBox(v)) => Some(*v),
            _ => None,
        });
        assert_eq!(b, Some(true));
    }

    #[test]
    fn unbox_value_of_after_new() {
        let h = __RTS_FN_GL_BOOLEAN_NEW(0) as i64;
        assert_eq!(__RTS_FN_GL_BOOLEAN_VALUE_OF(h), 0);
    }
}
