//! End-to-end check: `#[rts_namespace]` derives the spec + callable externs.

use rts_engine::abi::ty::{Handle, F64, I64};
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

    /// a constant member (no parens in TS), with an unused-param-free body
    #[rts_const(pure)]
    pub fn width() -> I64 {
        64
    }

    /// unused params keep a leading `_` but the TS name drops it
    #[rts_fn]
    pub fn pick_second(_a: I64, b: I64) -> I64 {
        b
    }
}

#[test]
fn derives_spec() {
    assert_eq!(SPEC.name, "toy");
    assert_eq!(SPEC.doc, "Toy namespace for the derive test.");
    assert_eq!(SPEC.members.len(), 7);

    let width = SPEC.members.iter().find(|m| m.name == "width").unwrap();
    assert!(matches!(width.kind, rts_engine::abi::MemberKind::Constant));
    assert_eq!(width.ts_signature, "width: number"); // no parens
    assert_eq!(__RTS_FN_NS_TOY_WIDTH(), 64);

    let pick = SPEC
        .members
        .iter()
        .find(|m| m.name == "pick_second")
        .unwrap();
    // leading `_` stripped in the TS name
    assert_eq!(
        pick.ts_signature,
        "pick_second(a: number, b: number): number"
    );
    assert_eq!(__RTS_FN_NS_TOY_PICK_SECOND(1, 2), 2);

    let bl = SPEC.members.iter().find(|m| m.name == "byte_len").unwrap();
    assert_eq!(bl.symbol, "__RTS_FN_NS_TOY_BYTE_LEN");
    assert_eq!(bl.ts_signature, "byte_len(s: string): number");
    assert!(matches!(bl.args[0], rts_engine::abi::AbiType::StrPtr));
    assert!(matches!(bl.returns, rts_engine::abi::AbiType::I64));

    let dbl = SPEC.members.iter().find(|m| m.name == "dbl").unwrap();
    assert_eq!(dbl.symbol, "__RTS_FN_NS_TOY_DBL");
    assert_eq!(dbl.ts_signature, "dbl(value: number): number");
    assert_eq!(dbl.doc, "double it");
    assert!(matches!(dbl.returns, rts_engine::abi::AbiType::I64));
    assert!(matches!(dbl.args[0], rts_engine::abi::AbiType::I64));
    assert!(!dbl.pure);

    let half = SPEC.members.iter().find(|m| m.name == "half").unwrap();
    assert_eq!(half.ts_signature, "half(value: number): number");
    assert!(half.pure);

    let ping = SPEC.members.iter().find(|m| m.name == "ping").unwrap();
    assert_eq!(ping.ts_signature, "ping(): void");
    assert!(matches!(ping.returns, rts_engine::abi::AbiType::Void));
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

/// `#[rts_module("scheme:name")]` + `#[rts_var]` end-to-end: the module name is
/// derived from the specifier, and the var emits callable atomic GET/SET externs
/// plus VarGetter/VarSetter members with the right flags.
mod modtest {
    use rts_engine::abi::ty::I64;
    use rts_macro::rts_module;

    /// Toy module declared by full specifier.
    #[rts_module("rts:toymod")]
    impl ToyMod {
        /// a plain fn member
        #[rts_fn]
        pub fn answer() -> I64 {
            42
        }

        /// a mutable process-global var (let/var) — getter + setter
        #[rts_var(var, I64, default = 5)]
        fn counter() {}

        /// a read-only var (const) — getter only, READONLY flag
        #[rts_var(const, I64, default = 100)]
        fn ceiling() {}
    }

    #[test]
    fn module_name_from_specifier() {
        assert_eq!(SPEC.name, "toymod");
    }

    #[test]
    fn var_externs_round_trip() {
        // const getter
        assert_eq!(__RTS_FN_NS_TOYMOD_CEILING_GET(), 100);
        // mutable var: read default, write, read back (process-global atomic)
        assert_eq!(__RTS_FN_NS_TOYMOD_COUNTER_GET(), 5);
        __RTS_FN_NS_TOYMOD_COUNTER_SET(99);
        assert_eq!(__RTS_FN_NS_TOYMOD_COUNTER_GET(), 99);
    }

    #[test]
    fn var_members_and_flags() {
        use rts_engine::abi::{MemberFlags, MemberKind};
        // answer (1) + counter get+set (2) + ceiling get (1) = 4 members
        assert_eq!(SPEC.members.len(), 4);

        let get = SPEC
            .members
            .iter()
            .find(|m| m.name == "counter" && matches!(m.kind, MemberKind::VarGetter))
            .unwrap();
        assert!(get.flags.contains(MemberFlags::MUTABLE));
        assert_eq!(get.symbol, "__RTS_FN_NS_TOYMOD_COUNTER_GET");

        let set = SPEC
            .members
            .iter()
            .find(|m| matches!(m.kind, MemberKind::VarSetter))
            .unwrap();
        assert_eq!(set.name, "counter");
        assert_eq!(set.symbol, "__RTS_FN_NS_TOYMOD_COUNTER_SET");

        let ceiling = SPEC
            .members
            .iter()
            .find(|m| m.name == "ceiling")
            .unwrap();
        assert!(matches!(ceiling.kind, MemberKind::VarGetter));
        assert!(ceiling.flags.contains(MemberFlags::READONLY));
        // const var emits no setter
        assert!(
            !SPEC
                .members
                .iter()
                .any(|m| m.name == "ceiling" && matches!(m.kind, MemberKind::VarSetter))
        );
    }
}
