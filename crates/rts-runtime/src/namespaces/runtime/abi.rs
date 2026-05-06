use crate::abi::{AbiType, MemberKind, NamespaceMember, NamespaceSpec};

pub const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "eval",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_RUNTIME_EVAL",
        args: &[AbiType::StrPtr],
        returns: AbiType::I64,
        doc: "Evaluates a TS/JS source string. Returns the program exit code.",
        ts_signature: "eval(src: string): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "eval_file",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_RUNTIME_EVAL_FILE",
        args: &[AbiType::StrPtr],
        returns: AbiType::I64,
        doc: "Loads and evaluates a TS/JS file at the given path. Returns the program exit code.",
        ts_signature: "eval_file(path: string): number",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "runtime",
    doc: "Dynamic TS/JS evaluation. JIT path uses inline compilation; AOT path spawns rts.",
    members: MEMBERS,
};
