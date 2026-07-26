//! The two remaining hand-written `String` statics: `String.fromCharCode(...)`
//! and `String.fromCodePoint(...)`. They are VARIADIC statics the value-class
//! macro cannot yet express (`#[rtse::statical]` is fixed-arity), so the engine's
//! variadic codegen path (`globals::string_static_call` → the `__rtsadp_str_*`
//! trampolines) calls these by address.
//!
//! The ENTIRE instance-method surface + the `new String(x)` wrapper migrated to
//! the pure-Rust primordial `String` value-class ([`super::value_class`], computed
//! via [`super::strops`]) — the legacy `__RTS_FN_GL_STRING_*` instance externs are
//! gone. `alloc_str` stays here as the shared pool-alloc helper (also used by
//! [`super::search`]).

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

pub(crate) fn alloc_str(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// ── Static methods (variadic — engine-direct via the codegen trampoline) ─────────

/// String.fromCharCode(code) — JS spec: trunca o argumento para u16
/// (mod 0x10000). Code unit isolado pode ser surrogate "orfao" — nao
/// eh codepoint UTF-8 valido. Nesse caso retorna string vazia (compat
/// com Bun/Node que produzem lone surrogate na string interna mas, ao
/// converter UTF-16 -> UTF-8 para output, replacement char ou vazio).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_FROM_CHAR_CODE(code: i64) -> u64 {
    let unit = (code as i32 as u32) & 0xFFFF;
    // Surrogate orfao (0xD800-0xDFFF) nao eh char valido. Retorna
    // string vazia (bate com output do Node ao converter para a
    // codificacao default).
    if (0xD800..=0xDFFF).contains(&unit) {
        return alloc_str("");
    }
    let ch = char::from_u32(unit).unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buf = [0u8; 4];
    alloc_str(ch.encode_utf8(&mut buf))
}

/// String.fromCodePoint(codePoint) — char from full Unicode code point.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_FROM_CODE_POINT(code: i64) -> u64 {
    let ch = char::from_u32(code as u32).unwrap_or(char::REPLACEMENT_CHARACTER);
    let mut buf = [0u8; 4];
    alloc_str(ch.encode_utf8(&mut buf))
}
