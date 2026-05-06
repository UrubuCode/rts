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
    // (#216) Well-known symbols — handles cacheados (mesma ref em chamadas
    // sucessivas). Adicionados como Constants pra uso em contextos de
    // metaprogramming (futuro Symbol.iterator dispatch em for-of, etc).
    NamespaceMember {
        name: "iterator",
        kind: MemberKind::Constant,
        symbol: "__RTS_FN_GL_SYMBOL_ITERATOR",
        args: &[],
        returns: AbiType::Handle,
        doc: "Symbol.iterator — well-known symbol pra iteration protocol.",
        ts_signature: "readonly iterator: unique symbol",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "asyncIterator",
        kind: MemberKind::Constant,
        symbol: "__RTS_FN_GL_SYMBOL_ASYNC_ITERATOR",
        args: &[],
        returns: AbiType::Handle,
        doc: "Symbol.asyncIterator — async iteration protocol.",
        ts_signature: "readonly asyncIterator: unique symbol",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "hasInstance",
        kind: MemberKind::Constant,
        symbol: "__RTS_FN_GL_SYMBOL_HAS_INSTANCE",
        args: &[],
        returns: AbiType::Handle,
        doc: "Symbol.hasInstance — controla instanceof.",
        ts_signature: "readonly hasInstance: unique symbol",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "toPrimitive",
        kind: MemberKind::Constant,
        symbol: "__RTS_FN_GL_SYMBOL_TO_PRIMITIVE",
        args: &[],
        returns: AbiType::Handle,
        doc: "Symbol.toPrimitive — controla coercao.",
        ts_signature: "readonly toPrimitive: unique symbol",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "toStringTag",
        kind: MemberKind::Constant,
        symbol: "__RTS_FN_GL_SYMBOL_TO_STRING_TAG",
        args: &[],
        returns: AbiType::Handle,
        doc: "Symbol.toStringTag — customiza Object.prototype.toString.",
        ts_signature: "readonly toStringTag: unique symbol",
        intrinsic: None,
        pure: true,
    },
];

pub const SYMBOL_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Symbol",
    doc: "Built-in Symbol primitive (#216). Each Symbol() call returns a unique handle.",
    members: MEMBERS,
};
