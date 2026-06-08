//! End-to-end check: `#[rts_namespace]` derives the spec + callable externs.

use rts_abi::ty::{Handle, F64, I64};
use rts_macro::rts_namespace;

// `Str` is a bare marker token recognised by the macro — it is never emitted
// as a real type, so it is intentionally NOT imported (importing it would be an
// unused import). The macro expands `s: Str` into `(s_ptr, s_len)`.

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

    /// byte length of a string (Str expands to ptr+len)
    #[rts_fn]
    pub fn byte_len(s: Str) -> I64 {
        s.len() as I64
    }

    /// fake intern: returns a non-zero handle when the string is non-empty
    #[rts_fn]
    pub fn intern(s: Str) -> Handle {
        if s.is_empty() {
            0
        } else {
            1
        }
    }
}

#[test]
fn derives_spec() {
    assert_eq!(SPEC.name, "toy");
    assert_eq!(SPEC.doc, "Toy namespace for the derive test.");
    assert_eq!(SPEC.members.len(), 5);

    let bl = SPEC.members.iter().find(|m| m.name == "byte_len").unwrap();
    assert_eq!(bl.symbol, "__RTS_FN_NS_TOY_BYTE_LEN");
    assert_eq!(bl.ts_signature, "byte_len(s: string): number");
    assert!(matches!(bl.args[0], rts_abi::AbiType::StrPtr));
    assert!(matches!(bl.returns, rts_abi::AbiType::I64));

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

#[test]
fn str_param_reconstructs_and_guards() {
    let s = "hello";
    // Valid (ptr, len) → reconstructed &str.
    assert_eq!(__RTS_FN_NS_TOY_BYTE_LEN(s.as_ptr(), s.len() as i64), 5);
    assert_eq!(__RTS_FN_NS_TOY_INTERN(s.as_ptr(), s.len() as i64), 1);
    // Null pointer → early-return default (0), no UB.
    assert_eq!(__RTS_FN_NS_TOY_BYTE_LEN(std::ptr::null(), 0), 0);
    assert_eq!(__RTS_FN_NS_TOY_INTERN(std::ptr::null(), 3), 0);
}
