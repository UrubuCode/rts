//! `Boolean` runtime — extern "C" implementations.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

/// Sentinels JS (em sync com sentinel_for em codegen):
/// MIN   = false, MIN+1 = true, MIN+2 = undefined, MIN+3 = null.
const FALSE_SENTINEL: i64 = i64::MIN;
const UNDEFINED_SENTINEL: i64 = i64::MIN + 2;
const NULL_SENTINEL: i64 = i64::MIN + 3;

/// `Boolean(x)` — coerce truthy/falsy. Implementacao pura em runtime para
/// quando o codegen nao consegue resolver inline (fallback).
///
/// Falsy: 0 (number/false-old-style), FALSE_SENTINEL, UNDEFINED_SENTINEL,
/// NULL_SENTINEL, empty string handle (Entry::String len==0), NaN (f64 bits).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_COERCE(value: i64) -> i64 {
    // (cross-runtime #1069) Sentinels JS — todos falsy.
    if value == 0
        || value == FALSE_SENTINEL
        || value == UNDEFINED_SENTINEL
        || value == NULL_SENTINEL
    {
        return 0;
    }
    // String handle vazio: detectar Entry::String len==0. NaN check
    // omitido — f64::from_bits sobre i64 negativo (ex: -1) tambem
    // produz NaN e classificaria incorretamente i64 puro negativo.
    if value > 0 {
        let h = value as u64;
        let is_empty_str = with_entry(h, |e| match e {
            Some(Entry::String(b)) => Some(b.is_empty()),
            _ => None,
        });
        if matches!(is_empty_str, Some(true)) {
            return 0;
        }
    }
    1
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

/// `new Boolean(x)` — aloca `Entry::BooleanBox` e retorna handle. `x` ja' vem
/// coerced (0 ou 1) pelo codegen.
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

/// `b.valueOf()` — retorna o proprio bool. Aceita handle boxed ou primitivo.
/// AbiType::Bool é mapeado para i64 em Cranelift (`scalar_to_cl`); retorno
/// é 0 ou 1, e o codegen interpreta como Bool semanticamente para typeof.
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
}
