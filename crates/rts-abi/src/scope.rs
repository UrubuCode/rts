//! Symbol DERIVATION — the single implementation of the naming rule.
//!
//! [`symbols`](crate::symbols) VALIDATES a name; this module PRODUCES one. The
//! convention (owner decision, 2026-07-27):
//!
//! ```text
//! __rtsm_<module>_<value>          module symbol    __rtsm_io_print, __rtsm_node_fs_readFile
//! __rtsm_global_<Class>_<value>    global class     __rtsm_global_String_toUpperCase
//! __rtsm_global_<value>            bare global      __rtsm_global_parseInt
//! __rtsn_<value>                   NATIVE           Rust helpers compensating what Cranelift
//!                                                   lacks. The `n` is NATIVE, NOT "node".
//! __rtsa_<value>                   ABI              the codegen<->runtime contract.
//! ```
//!
//! Two rules make the name mechanical:
//!
//! - The separator is ALWAYS `_`, never a hyphen. A module specifier or class
//!   name is normalized into segments by [`segments`]: `node:fs` → `node_fs`,
//!   `Intl.NumberFormat` → `Intl_NumberFormat`.
//! - **Every segment keeps its case** — the scope one as much as the trailing
//!   `<value>`. Folding the member would collide JS members differing only by
//!   case; folding the CLASS destroyed the word boundaries of a PascalCase name
//!   (`readablestreamdefaultcontroller`), so the symbol could not be read back.
//!   Both are verbatim, which makes the whole name reversible.
//!
//! # Why this lives in `rts-abi` and not in the proc-macro
//!
//! `#[rtse::abi]` derives the symbol at expansion time, and `rts-symbol-baker`
//! derives the SAME symbol at bake time from the same source text. A proc-macro
//! crate cannot be depended on as an ordinary library, so if the rule stayed in
//! `rts-macro` the baker would have to re-implement it — reinstating exactly the
//! name drift this convention exists to delete. `rts-abi` is dependency-free and
//! sits at the bottom of the graph, so both sides can call one function.

/// How a declaration's symbol is named.
///
/// This is the semantic content of `#[rtse::abi(...)]`, with the `syn` parsing
/// left in the proc-macro (a `Parse` impl needs a local type). The macro wraps
/// this in a newtype and delegates the naming here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Naming {
    /// No arguments — the symbol is the Rust fn name prefixed with `__`. The
    /// transitional form for symbols whose spelling predates the convention;
    /// drained as each group is converted to a declared scope.
    Verbatim,
    /// An explicit spelling the rule would not produce. The escape hatch, not
    /// the norm.
    Explicit(String),
    /// A declared scope; the symbol is derived from it plus `value`.
    Scoped { scope: Scope, value: Option<String> },
}

/// Which of the prefixes a declaration belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// `module = "node:fs"` → `__rtsm_node_fs_<value>`.
    Module(String),
    /// `global = "String"` → `__rtsm_global_String_<value>`; bare `global` (no
    /// class) → `__rtsm_global_<value>`.
    Global(Option<String>),
    /// `native` → `__rtsn_<value>`.
    Native,
    /// `abi` → `__rtsa_<value>`.
    Abi,
}

/// Compose the symbol's trailing segment for an ARITY OVERLOAD.
///
/// JS lets several members share one name and differ by arity; a linker symbol
/// cannot. `value` is the JS name (the same for every overload), `overload` is
/// the author-written disambiguator, and the symbol carries both — exactly the
/// shape the hand-written names already used (`__RTS_FN_NODE_ZLIB_CRC32` and
/// `__RTS_FN_NODE_ZLIB_CRC32_PREV`).
///
/// ONE implementation, called by `#[rtse::function]` at expansion time and by
/// `rts-symbol-baker` at bake time — the same reason `symbol_for` lives here.
pub fn with_overload(value: String, overload: Option<&str>) -> String {
    match overload {
        Some(o) => format!("{value}_{o}"),
        None => value,
    }
}

/// The linker symbol for `naming` on a fn named `fn_name`.
///
/// `value` defaults to the Rust fn name when omitted, so a helper whose Rust
/// name already IS its JS name declares only its scope.
pub fn symbol_for(naming: &Naming, fn_name: &str) -> String {
    match naming {
        Naming::Verbatim => format!("__{fn_name}"),
        Naming::Explicit(s) => s.clone(),
        Naming::Scoped { scope, value } => {
            let v = value.clone().unwrap_or_else(|| fn_name.to_string());
            match scope {
                Scope::Module(m) => format!("__rtsm_{}_{}", segments(m), v),
                Scope::Global(Some(c)) => format!("__rtsm_global_{}_{}", segments(c), v),
                Scope::Global(None) => format!("__rtsm_global_{v}"),
                Scope::Native => format!("__rtsn_{v}"),
                Scope::Abi => format!("__rtsa_{v}"),
            }
        }
    }
}

/// Normalize a module specifier or class name into underscore-joined symbol
/// segments: every run of non-alphanumeric characters collapsed to a single
/// `_`, no leading/trailing separator. `node:fs` → `node_fs`,
/// `Intl.NumberFormat` → `Intl_NumberFormat`, `text-encoding` → `text_encoding`.
///
/// **Case is PRESERVED** (owner decision, 2026-07-31). It used to be folded to
/// lower, which destroyed the word boundaries of a PascalCase class name: the
/// symbol read `__rtsm_global_ReadableStreamDefaultController_close`, from which
/// you cannot recover `ReadableStreamDefaultController`. Preserving the case
/// makes the symbol REVERSIBLE — scope and member are both readable straight off
/// the linker name — and it makes the rule uniform, since the `<value>` segment
/// was already verbatim. A module specifier is lower-case in JS anyway, so this
/// changes nothing for `node:fs`/`io`; it is the class segment that gains.
pub fn segments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            if pending && !out.is_empty() {
                out.push('_');
            }
            pending = false;
            out.push(c);
        } else {
            pending = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_collapse_to_underscores() {
        assert_eq!(segments("node:fs"), "node_fs");
        assert_eq!(segments("Intl.NumberFormat"), "Intl_NumberFormat");
        assert_eq!(segments("text-encoding"), "text_encoding");
        assert_eq!(segments("io"), "io");
    }

    #[test]
    fn symbols_follow_the_convention() {
        let m = Naming::Scoped {
            scope: Scope::Module("node:fs".into()),
            value: Some("readFileSync".into()),
        };
        assert_eq!(symbol_for(&m, "read_file_sync"), "__rtsm_node_fs_readFileSync");

        let g = Naming::Scoped {
            scope: Scope::Global(Some("String".into())),
            value: Some("toUpperCase".into()),
        };
        assert_eq!(symbol_for(&g, "upper"), "__rtsm_global_String_toUpperCase");

        let bare = Naming::Scoped {
            scope: Scope::Global(None),
            value: Some("parseInt".into()),
        };
        assert_eq!(symbol_for(&bare, "x"), "__rtsm_global_parseInt");

        // `value` defaults to the Rust fn name.
        let n = Naming::Scoped {
            scope: Scope::Native,
            value: None,
        };
        assert_eq!(symbol_for(&n, "fmod"), "__rtsn_fmod");

        let a = Naming::Scoped {
            scope: Scope::Abi,
            value: Some("obj_get".into()),
        };
        assert_eq!(symbol_for(&a, "x"), "__rtsa_obj_get");
    }

    #[test]
    fn verbatim_prefixes_the_rust_name() {
        assert_eq!(symbol_for(&Naming::Verbatim, "rtsadp_obj_get"), "__rtsadp_obj_get");
    }

    #[test]
    fn derived_names_pass_validation() {
        let m = Naming::Scoped {
            scope: Scope::Module("node:fs".into()),
            value: Some("readFileSync".into()),
        };
        assert!(crate::symbols::validate_symbol(&symbol_for(&m, "x")).is_ok());
    }
}
