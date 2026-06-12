//! `Boolean` global class (#208 #226) — coercao truthy/falsy + toString/valueOf.
//!
//! PRIMORDIAL (Fase 2): movido de `rts-shared/globals/boolean` p/ `rts-primitives`.
//! Os externs `__RTS_FN_GL_BOOLEAN_*` + `register_boolean_class_spec()` são
//! escritos à mão. Depende só de `rts-engine`.

use rts_engine::abi::ty::{Bool, Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

// `string_pool` (gc-surface) fica no `rts-runtime` collector; símbolo
// `#[no_mangle]` resolve em link-time (JIT registra; AOT pelo staticlib).
unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

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

/// new Boolean(value) — boxed boolean object. typeof === 'object'.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_NEW(value: I64) -> Handle {
    let b = value != 0 && value != UNDEFINED_SENTINEL;
    alloc_entry(Entry::BooleanBox(b))
}

/// new Boolean() — boxed Boolean(false).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_NEW_EMPTY() -> Handle {
    alloc_entry(Entry::BooleanBox(false))
}

/// Coerces any value to boolean (truthy/falsy). 0 / NaN bits / undefined sentinel → false.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_COERCE(value: I64) -> I64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_TO_STRING(b: I64) -> Handle {
    let s: &[u8] = if unbox_bool(b) { b"true" } else { b"false" };
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Returns the underlying boolean value.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BOOLEAN_VALUE_OF(b: I64) -> Bool {
    if unbox_bool(b) {
        1
    } else {
        0
    }
}

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a classe global `Boolean` no motor (Fase 2 — hand-written, sem macro).
pub fn register_boolean_class_spec(e: &mut Engine) {
    e.class("Boolean")
        .doc("Built-in Boolean primitive (#208 #226). Boolean(x) coerces any value to boolean.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_BOOLEAN_NEW",
            "new(value: any): Boolean",
            "new Boolean(value) — boxed boolean object. typeof === 'object'.",
            __RTS_FN_GL_BOOLEAN_NEW as *const u8,
            true,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_BOOLEAN_NEW_EMPTY",
            "new(): Boolean",
            "new Boolean() — boxed Boolean(false).",
            __RTS_FN_GL_BOOLEAN_NEW_EMPTY as *const u8,
            true,
        ))
        .member(m(
            "coerce",
            MemberKind::Function,
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_BOOLEAN_COERCE",
            "coerce(value: any): boolean",
            "Coerces any value to boolean (truthy/falsy). 0 / NaN bits / undefined sentinel → false.",
            __RTS_FN_GL_BOOLEAN_COERCE as *const u8,
            true,
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_BOOLEAN_TO_STRING",
            "toString(): string",
            "Returns 'true' or 'false' string handle.",
            __RTS_FN_GL_BOOLEAN_TO_STRING as *const u8,
            true,
        ))
        .member(m(
            "valueOf",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::I64], AbiType::Bool),
            "__RTS_FN_GL_BOOLEAN_VALUE_OF",
            "valueOf(): boolean",
            "Returns the underlying boolean value.",
            __RTS_FN_GL_BOOLEAN_VALUE_OF as *const u8,
            true,
        ))
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::{with_entry, Entry};

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
