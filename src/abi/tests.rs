//! Cross-cutting invariants of the new ABI registry.
//!
//! These tests run on every `cargo test` invocation and catch mistakes that
//! single-module tests miss: duplicate symbols, malformed names,
//! non-returnable types in return slots, and stale member/signature links.

#![cfg(test)]

use std::collections::HashSet;

use crate::abi::SPECS;
use crate::abi::member::{MemberKind, NamespaceSpec};
use crate::abi::symbols::validate_symbol;
use crate::abi::types::AbiType;

#[test]
fn every_symbol_is_canonical() {
    for spec in SPECS {
        for member in spec.members {
            assert!(
                validate_symbol(member.symbol).is_ok(),
                "member {} has malformed symbol {}",
                member.name,
                member.symbol
            );
        }
    }
}

#[test]
fn symbols_are_globally_unique() {
    // Mesmo simbolo so' eh permitido quando duas specs declaram um alias
    // (assinatura identica). Ex: `JSON.parse` (global) e `json.parse`
    // (namespace) compartilham `__RTS_FN_NS_JSON_PARSE`. Colisao com
    // assinaturas divergentes continua sendo erro.
    use std::collections::HashMap;
    let mut seen: HashMap<&'static str, (&'static str, &'static [AbiType], AbiType)> =
        HashMap::new();
    for spec in SPECS {
        for member in spec.members {
            let sig = (spec.name, member.args, member.returns);
            if let Some(prev) = seen.get(member.symbol) {
                assert!(
                    prev.1 == sig.1 && prev.2 == sig.2,
                    "symbol {} shared between {} and {} with diverging signatures",
                    member.symbol,
                    prev.0,
                    spec.name
                );
            } else {
                seen.insert(member.symbol, sig);
            }
        }
    }
}

#[test]
fn namespace_names_are_unique() {
    let mut seen: HashSet<&'static str> = HashSet::new();
    for spec in SPECS {
        assert!(
            seen.insert(spec.name),
            "duplicate namespace name {}",
            spec.name
        );
    }
}

#[test]
fn returns_are_returnable() {
    for spec in SPECS {
        for member in spec.members {
            assert!(
                member.returns.is_returnable(),
                "member {}.{} has non-returnable return type {:?}",
                spec.name,
                member.name,
                member.returns
            );
        }
    }
}

#[test]
fn constants_have_no_args() {
    for spec in SPECS {
        for member in spec.members {
            if matches!(member.kind, MemberKind::Constant) {
                assert!(
                    member.args.is_empty(),
                    "constant {}.{} must have zero args",
                    spec.name,
                    member.name
                );
                assert!(
                    !matches!(member.returns, AbiType::Void),
                    "constant {}.{} cannot return Void",
                    spec.name,
                    member.name
                );
            }
        }
    }
}

#[test]
fn specs_expose_expected_shape() {
    // Sanity check that SPECS type is the expected slice shape so future
    // migrations that append a `NamespaceSpec` do not change the type by
    // mistake. The registry is allowed to be empty on introduction.
    let _: &[&NamespaceSpec] = SPECS;
}
