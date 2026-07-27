//! Symbol naming convention for the ABI boundary.
//!
//! Every exported runtime entry point takes one of four forms (owner decision,
//! 2026-07-27):
//!
//! ```text
//! __rtsm_<module>_<value>          module symbol    __rtsm_io_print, __rtsm_node_fs_readFile
//! __rtsm_global_<class>_<value>    global class     __rtsm_global_string_toUpperCase
//! __rtsm_global_<value>            bare global      __rtsm_global_parseInt
//! __rtsn_<value>                   NATIVE           Rust helpers compensating what Cranelift
//!                                                   lacks (arithmetic/computation LLVM has).
//!                                                   The `n` is NATIVE — it is NOT "node".
//! __rtsa_<value>                   ABI              the codegen<->runtime contract.
//! ```
//!
//! Three rules define the shape:
//!
//! - Segments are joined with `_`, NEVER a hyphen. A module specifier is
//!   normalized into segments (`node:fs` → `node_fs`).
//! - There are no uppercase block segments. `FN`, `NS`, `GL`, `GC` and `CONST`
//!   are gone — the prefix carries everything they used to encode, and the
//!   superseded `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` form is rejected outright
//!   ([`SymbolError::LegacyForm`]).
//! - The trailing `<value>` segment is the JS member spelling VERBATIM
//!   (`readFileSync`, `toUpperCase`). Case is deliberately preserved: folding it
//!   collides distinct JS members that differ only by case.
//!
//! Symbols are not written by hand — `#[rtse::abi]` derives them from a declared
//! scope (see `rts-macro`'s `abi::scope`). [`validate_symbol`] checks the shape
//! of a name that reaches the Registry from anywhere else.

/// Returns `Ok(())` when `symbol` matches the canonical format.
///
/// Kept as a regular function (rather than `const`) so it can be invoked from
/// tests and debug assertions. Performance is irrelevant — validation runs
/// once during `SPECS` iteration at startup.
pub fn validate_symbol(symbol: &str) -> Result<(), SymbolError> {
    if symbol.starts_with("__RTS_") {
        return Err(SymbolError::LegacyForm);
    }
    let (tail, min_segments) = if let Some(t) = symbol.strip_prefix("__rtsm_") {
        // A module symbol always carries a scope AND a value (`io` + `print`);
        // a bare `__rtsm_print` names no module and is not resolvable.
        (t, 2)
    } else if let Some(t) = symbol
        .strip_prefix("__rtsn_")
        .or_else(|| symbol.strip_prefix("__rtsa_"))
    {
        (t, 1)
    } else {
        return Err(SymbolError::MissingPrefix);
    };

    if tail.is_empty() {
        return Err(SymbolError::MissingName);
    }
    // Case-PRESERVED: ASCII letters of either case, digits, and `_` as the
    // segment separator. Nothing else — a hyphen or a `:` means the scope was
    // not normalized into segments.
    if !tail
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(SymbolError::InvalidCharacter);
    }
    if tail.split('_').filter(|s| !s.is_empty()).count() < min_segments {
        return Err(SymbolError::MissingName);
    }

    Ok(())
}

/// Structured error produced by [`validate_symbol`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolError {
    /// Not one of `__rtsm_` / `__rtsn_` / `__rtsa_`.
    MissingPrefix,
    /// The superseded `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` convention.
    LegacyForm,
    /// Nothing after the prefix, or a module symbol with no scope segment.
    MissingName,
    /// A character outside `[A-Za-z0-9_]` — typically an unnormalized `:`/`-`.
    InvalidCharacter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_canonical_names() {
        assert!(validate_symbol("__rtsm_io_print").is_ok());
        assert!(validate_symbol("__rtsm_node_fs_readFileSync").is_ok());
        assert!(validate_symbol("__rtsm_global_string_toUpperCase").is_ok());
        assert!(validate_symbol("__rtsm_global_parseInt").is_ok());
        assert!(validate_symbol("__rtsn_fmod").is_ok());
        assert!(validate_symbol("__rtsa_obj_get").is_ok());
    }

    #[test]
    fn rejects_the_superseded_convention() {
        assert_eq!(
            validate_symbol("__RTS_FN_NS_IO_PRINT"),
            Err(SymbolError::LegacyForm)
        );
        assert_eq!(
            validate_symbol("__RTS_FN_GL_STRING_toUpperCase"),
            Err(SymbolError::LegacyForm)
        );
    }

    #[test]
    fn rejects_malformed_names() {
        assert_eq!(validate_symbol("rtsm_io_print"), Err(SymbolError::MissingPrefix));
        assert_eq!(
            validate_symbol("__rtsadp_obj_get"),
            Err(SymbolError::MissingPrefix)
        );
        assert_eq!(validate_symbol("__rtsa_"), Err(SymbolError::MissingName));
        // A module symbol needs a scope segment as well as a value.
        assert_eq!(validate_symbol("__rtsm_print"), Err(SymbolError::MissingName));
        // A specifier that was not normalized into `_` segments.
        assert_eq!(
            validate_symbol("__rtsm_node:fs_readFile"),
            Err(SymbolError::InvalidCharacter)
        );
        assert_eq!(
            validate_symbol("__rtsm_io_pr!nt"),
            Err(SymbolError::InvalidCharacter)
        );
    }
}
