//! Rust-type token → [`AbiType`] — the single implementation of the mapping.
//!
//! [`scope`](crate::scope) derives a symbol's NAME from its declaration; this
//! module derives its ABI TYPES. Both live here, dependency-free at the bottom
//! of the graph, for the same reason: **two independent consumers must agree.**
//!
//! - `#[rtse::abi]` maps the Rust signature at EXPANSION time to emit the
//!   `SymbolDesc` const.
//! - `rts-symbol-baker` maps the SAME signature at BAKE time, reading the same
//!   source text, to emit the signature table.
//!
//! A proc-macro crate cannot be depended on as an ordinary library, so if the
//! mapping stayed in `rts-macro` the baker would have to re-implement it —
//! reinstating exactly the drift this exists to delete. (`scope::symbol_for`
//! carries the identical rationale for names.)
//!
//! # The mapping is on the WRITTEN TOKEN, deliberately
//!
//! Several ABI types share one Rust repr: `Handle`, `U64` and `Poly` are all
//! `u64`; `Bool` and `I64` are both an `i64` slot. A type-resolving pass could
//! not tell them apart — but the AUTHOR can, and does, by writing the alias
//! whose name carries the intent. So this maps the token as written, and an
//! author who writes `u64` where they meant `Handle` gets `U64`. That is
//! harmless for signature LOWERING (both are one `i64` slot; see
//! `signature::lower_params`) and is the reason converting a hand-written
//! `Handle` row to a derived `U64` one is behaviour-preserving.

use crate::types::AbiType;

/// The [`AbiType`] for a single-slot scalar/handle type, by written token.
/// `None` for anything with no single-slot spelling (a `&str` is `StrPtr` and
/// takes TWO slots — see [`is_str_token`]; everything else is unsupported).
pub fn single_slot(token: &str) -> Option<AbiType> {
    Some(match token {
        "u64" | "U64" => AbiType::U64,
        "Handle" => AbiType::Handle,
        "Poly" => AbiType::PolyValue,
        "i64" | "I64" => AbiType::I64,
        "i32" | "I32" => AbiType::I32,
        "f64" | "F64" => AbiType::F64,
        "bool" | "Bool" => AbiType::Bool,
        _ => return None,
    })
}

/// Is this written token a string parameter (`&str` / `str` / `String`)? Such a
/// parameter is ONE [`AbiType::StrPtr`] occupying TWO ABI slots (`ptr` + `len`).
pub fn is_str_token(token: &str) -> bool {
    matches!(token, "str" | "String")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_sharing_a_repr_stay_distinct() {
        // The whole point: same Rust repr, different declared intent.
        assert_eq!(single_slot("u64"), Some(AbiType::U64));
        assert_eq!(single_slot("Handle"), Some(AbiType::Handle));
        assert_eq!(single_slot("Poly"), Some(AbiType::PolyValue));
        assert_eq!(single_slot("bool"), Some(AbiType::Bool));
        assert_eq!(single_slot("i64"), Some(AbiType::I64));
    }

    #[test]
    fn unknown_token_is_none_not_a_guess() {
        assert_eq!(single_slot("Vec"), None);
        assert_eq!(single_slot("*const u8"), None);
        assert!(is_str_token("str"));
        assert!(!is_str_token("u64"));
    }
}
