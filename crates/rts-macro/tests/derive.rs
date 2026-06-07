//! End-to-end check: `#[rts_namespace]` derives the spec + callable externs.

use rts_abi::ty::{F64, I64};
use rts_macro::rts_namespace;

/// Toy namespace for the derive test.
#[rts_namespace(toy)]
impl Toy {
    /// double it
    #[rts_fn]
    pub fn dbl(value: I64) -> I64 {
        value * 2
    }

    /// half, with an explicit TS signature override
    #[rts_fn(ts = "half(value: number): number", pure)]
    pub fn half(value: F64) -> F64 {
        value / 2.0
    }

    /// no-arg, no-return
    #[rts_fn]
    pub fn ping() {
        let _ = 1 + 1;
    }
}

#[test]
fn derives_spec() {
    assert_eq!(SPEC.name, "toy");
    assert_eq!(SPEC.doc, "Toy namespace for the derive test.");
    assert_eq!(SPEC.members.len(), 3);

    let dbl = SPEC.members.iter().find(|m| m.name == "dbl").unwrap();
    assert_eq!(dbl.symbol, "__RTS_FN_NS_TOY_DBL");
    assert_eq!(dbl.ts_signature, "dbl(value: number): number");
    assert_eq!(dbl.doc, "double it");
    assert!(matches!(dbl.returns, rts_abi::AbiType::I64));
    assert!(matches!(dbl.args[0], rts_abi::AbiType::I64));
    assert!(!dbl.pure);

    let half = SPEC.members.iter().find(|m| m.name == "half").unwrap();
    assert_eq!(half.ts_signature, "half(value: number): number");
    assert!(half.pure);

    let ping = SPEC.members.iter().find(|m| m.name == "ping").unwrap();
    assert_eq!(ping.ts_signature, "ping(): void");
    assert!(matches!(ping.returns, rts_abi::AbiType::Void));
}

#[test]
fn externs_are_callable() {
    assert_eq!(__RTS_FN_NS_TOY_DBL(21), 42);
    assert_eq!(__RTS_FN_NS_TOY_HALF(10.0), 5.0);
}
