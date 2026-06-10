//! `Boolean` global class (#208 #226) — coercao truthy/falsy + toString/valueOf.
//!
//! Migrado ao modelo `#[rts_class]` (stage 5, `docs/specs/rts-core-engine.md`):
//! um unico `impl` declara construtores, static e instance methods; o macro
//! deriva os externs `__RTS_FN_GL_BOOLEAN_*` + o `BOOLEAN_CLASS_SPEC`.

use rts_engine::abi::ty::{Bool, Handle, I64};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

/// Sentinels JS (em sync com sentinel_for em codegen):
/// MIN   = false, MIN+1 = true, MIN+2 = undefined, MIN+3 = null.
const FALSE_SENTINEL: i64 = i64::MIN;
const UNDEFINED_SENTINEL: i64 = i64::MIN + 2;
const NULL_SENTINEL: i64 = i64::MIN + 3;

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

/// Built-in Boolean primitive (#208 #226). Boolean(x) coerces any value to boolean.
#[rts_class(Boolean)]
impl BooleanClass {
    /// new Boolean(value) — boxed boolean object. typeof === 'object'.
    #[rts_ctor(ts = "new(value: any): Boolean", pure)]
    pub fn new(value: I64) -> Handle {
        let b = value != 0 && value != UNDEFINED_SENTINEL;
        alloc_entry(Entry::BooleanBox(b))
    }

    /// new Boolean() — boxed Boolean(false).
    #[rts_ctor(ts = "new(): Boolean", pure)]
    pub fn new_empty() -> Handle {
        alloc_entry(Entry::BooleanBox(false))
    }

    /// Coerces any value to boolean (truthy/falsy). 0 / NaN bits / undefined sentinel → false.
    #[rts_fn(ts = "coerce(value: any): boolean", pure)]
    pub fn coerce(value: I64) -> I64 {
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

    /// Returns 'true' or 'false' string handle.
    #[rts_method(name = "toString", ts = "toString(): string", pure)]
    pub fn to_string(b: I64) -> Handle {
        let s: &[u8] = if unbox_bool(b) { b"true" } else { b"false" };
        crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64)
    }

    /// Returns the underlying boolean value.
    #[rts_method(name = "valueOf", ts = "valueOf(): boolean", pure)]
    pub fn value_of(b: I64) -> Bool {
        if unbox_bool(b) {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespaces::gc::handles::{with_entry, Entry};

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
