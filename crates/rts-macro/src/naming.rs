//! Symbol/const name derivation.
//!
//! Two independent naming schemes live in this crate — `#[rtse::class]`'s
//! `__RTS_FN_GL_<CLASS>_<MEMBER>` convention (derived inline in
//! `class::member`, next to the code that needs the exact casing rules) and
//! `#[rtse::abi]`'s `SymbolDesc` const name — but `to_camel` (JS member-name
//! casing) is shared by both class members and struct fields, so it lives here
//! rather than under `class/`.

/// snake_case → camelCase, for a Rust member/field name with no explicit
/// `name = "..."` override.
pub(crate) fn to_camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.push(c.to_ascii_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The generated const's identifier for symbol `sym`: strip the engine prefix,
/// upper-case, suffix `_ABI`. `__rtsadp_obj_get` → `OBJ_GET_ABI`;
/// `__RTS_FN_NS_IO_PRINT` → `NS_IO_PRINT_ABI`.
pub(crate) fn abi_const_name(sym: &str) -> String {
    let base = sym
        .strip_prefix("__rtsadp_")
        .or_else(|| sym.strip_prefix("__RTS_FN_"))
        .or_else(|| sym.strip_prefix("__RTS_"))
        .unwrap_or(sym);
    format!("{}_ABI", base.trim_start_matches('_').to_uppercase())
}
