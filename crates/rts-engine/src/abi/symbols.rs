//! Symbol naming convention for the ABI boundary.
//!
//! All exported runtime entry points use the form:
//!
//! ```text
//! __RTS_<KIND>_<SCOPE>_<NS>_<NAME>
//! ```
//!
//! where `KIND` is one of `FN`, `CONST`, `TYPE`, `SCOPE` is one of `NS`,
//! `GC`, `ABI`, `GL` (global JS objects/classes), and both `NS` and `NAME`
//! are uppercase ASCII with digits and
//! underscores. The convention is enforced at startup by
//! [`validate_symbol`]; malformed entries in `SPECS` cause immediate panic
//! during tests and in debug builds.

/// Build a symbol name at compile time.
///
/// ```ignore
/// const SYM: &str = rts_sym!(FN NS IO PRINT);
/// assert_eq!(SYM, "__RTS_FN_NS_IO_PRINT");
/// ```
#[macro_export]
macro_rules! rts_sym {
    (FN NS $ns:ident $name:ident) => {
        concat!("__RTS_FN_NS_", stringify!($ns), "_", stringify!($name))
    };
    (FN GC $name:ident) => {
        concat!("__RTS_FN_GC_", stringify!($name))
    };
    (FN ABI $name:ident) => {
        concat!("__RTS_FN_ABI_", stringify!($name))
    };
    (CONST NS $ns:ident $name:ident) => {
        concat!("__RTS_CONST_NS_", stringify!($ns), "_", stringify!($name))
    };
}

/// Returns `Ok(())` when `symbol` matches the canonical format.
///
/// Kept as a regular function (rather than `const`) so it can be invoked from
/// tests and debug assertions. Performance is irrelevant — validation runs
/// once during `SPECS` iteration at startup.
pub fn validate_symbol(symbol: &str) -> Result<(), SymbolError> {
    let rest = symbol
        .strip_prefix("__RTS_")
        .ok_or(SymbolError::MissingPrefix)?;

    let mut parts = rest.splitn(2, '_');
    let kind = parts.next().ok_or(SymbolError::MissingKind)?;
    if !matches!(kind, "FN" | "CONST" | "TYPE") {
        return Err(SymbolError::InvalidKind);
    }

    let rest = parts.next().ok_or(SymbolError::MissingScope)?;
    let mut parts = rest.splitn(2, '_');
    let scope = parts.next().ok_or(SymbolError::MissingScope)?;
    if !matches!(scope, "NS" | "GC" | "ABI" | "GL") {
        return Err(SymbolError::InvalidScope);
    }

    let tail = parts.next().ok_or(SymbolError::MissingName)?;
    if tail.is_empty() {
        return Err(SymbolError::MissingName);
    }
    if !tail
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(SymbolError::InvalidCharacter);
    }

    Ok(())
}

/// Nome do entry point sintetico do top-level (`__RTS_MAIN`).
///
/// Centralizado para evitar drift entre codegen, JIT loader, eval_jit e
/// pipeline (#283). Mudar aqui propaga pra todos os call sites.
pub const ENTRY_POINT: &str = "__RTS_MAIN";

/// Structured error produced by [`validate_symbol`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolError {
    MissingPrefix,
    MissingKind,
    InvalidKind,
    MissingScope,
    InvalidScope,
    MissingName,
    InvalidCharacter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_emits_expected_format() {
        const S: &str = rts_sym!(FN NS IO PRINT);
        assert_eq!(S, "__RTS_FN_NS_IO_PRINT");

        const C: &str = rts_sym!(CONST NS PROCESS PLATFORM);
        assert_eq!(C, "__RTS_CONST_NS_PROCESS_PLATFORM");

        const G: &str = rts_sym!(FN GC STRING_NEW);
        assert_eq!(G, "__RTS_FN_GC_STRING_NEW");
    }

    #[test]
    fn validates_canonical_names() {
        assert!(validate_symbol("__RTS_FN_NS_IO_PRINT").is_ok());
        assert!(validate_symbol("__RTS_FN_GC_ARRAY_PUSH").is_ok());
        assert!(validate_symbol("__RTS_CONST_NS_PROCESS_PLATFORM").is_ok());
        assert!(validate_symbol("__RTS_TYPE_ABI_STRING_HANDLE").is_ok());
    }

    #[test]
    fn rejects_malformed_names() {
        assert_eq!(
            validate_symbol("rts_fn_ns_io_print"),
            Err(SymbolError::MissingPrefix)
        );
        assert_eq!(
            validate_symbol("__RTS_foo_NS_IO_PRINT"),
            Err(SymbolError::InvalidKind)
        );
        assert_eq!(
            validate_symbol("__RTS_FN_XX_IO_PRINT"),
            Err(SymbolError::InvalidScope)
        );
        assert_eq!(
            validate_symbol("__RTS_FN_NS_IO_print"),
            Err(SymbolError::InvalidCharacter)
        );
        assert_eq!(
            validate_symbol("__RTS_FN_NS_"),
            Err(SymbolError::MissingName)
        );
    }
}
