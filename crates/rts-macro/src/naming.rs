//! Symbol/const name derivation.
//!
//! There is ONE symbol rule, in `abi::scope`; `#[rtse::class]` and
//! `#[rtse::constant]` call into it (`class::class_symbol`) rather than restating
//! it. What lives here is the naming that is NOT the symbol rule: `to_camel` (JS
//! member-name casing, shared by class members, struct fields and constants) and
//! `abi_const_name` (the generated `SymbolDesc` const's Rust identifier).
//!
//! The symbol convention itself is derived in `abi::scope`:
//! `__rtsm_<module>_<value>` / `__rtsm_global_<class>_<value>` for module and
//! global-class symbols, `__rtsn_<value>` for NATIVE helpers, `__rtsa_<value>`
//! for the codegen<->runtime ABI contract.

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

/// The generated const's identifier for symbol `sym`: strip the scope prefix,
/// upper-case, suffix `_ABI`. `__rtsa_obj_get` → `OBJ_GET_ABI`;
/// `__rtsm_node_fs_readFileSync` → `NODE_FS_READFILESYNC_ABI`.
///
/// A symbol carrying a spelling from before the convention (the transitional
/// no-argument form of `#[rtse::abi]`) keeps its own leading underscores
/// stripped and is upper-cased the same way, so `__rtsadp_obj_get` also lands
/// on `OBJ_GET_ABI`.
pub(crate) fn abi_const_name(sym: &str) -> String {
    let base = sym
        .strip_prefix("__rtsm_")
        .or_else(|| sym.strip_prefix("__rtsn_"))
        .or_else(|| sym.strip_prefix("__rtsa_"))
        .or_else(|| sym.strip_prefix("__rtsadp_"))
        .unwrap_or(sym);
    format!("{}_ABI", base.trim_start_matches('_').to_uppercase())
}
