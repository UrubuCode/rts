//! `Symbol` global class — ABI declarativa.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    // ── Constructor ──────────────────────────────────────────────────────────
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_SYMBOL_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Creates a new unique Symbol with optional description string.",
        ts_signature: "new Symbol(description?: string): symbol",
        intrinsic: None,
        pure: false,
    },
    // ── Static methods ───────────────────────────────────────────────────────
    NamespaceMember {
        name: "for",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_SYMBOL_FOR",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Returns a registered symbol by key — same key always returns same handle.",
        ts_signature: "for(key: string): symbol",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "keyFor",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_SYMBOL_KEY_FOR",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns the key for a registered symbol, or 0 (undefined) if not registered.",
        ts_signature: "keyFor(sym: symbol): string | undefined",
        intrinsic: None,
        pure: true,
    },
    // ── Instance ─────────────────────────────────────────────────────────────
    NamespaceMember {
        name: "description",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_SYMBOL_DESCRIPTION",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns the symbol's description string, or 0 if none.",
        ts_signature: "description: string | undefined",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_SYMBOL_TO_STRING",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns 'Symbol(description)' string.",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
];

pub const SYMBOL_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Symbol",
    doc: "Built-in Symbol primitive (#216). Each Symbol() call returns a unique handle.",
    members: MEMBERS,
};
