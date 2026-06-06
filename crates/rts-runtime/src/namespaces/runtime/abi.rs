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
    NamespaceMember {
        name: "import_module",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_RUNTIME_IMPORT_MODULE",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Dynamic `import(path)`: compiles+runs the module in-process and \
              returns a handle to its exports namespace object (cached per path).",
        ts_signature: "import_module(path: string): number",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "set_module_exports",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_RUNTIME_SET_MODULE_EXPORTS",
        args: &[AbiType::Handle],
        returns: AbiType::Void,
        doc: "Internal: a dynamically-imported module registers its exports \
              namespace handle here so the importer can retrieve it.",
        ts_signature: "set_module_exports(ns: number): void",
        intrinsic: None,
        pure: false,
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "runtime",
    doc: "Dynamic TS/JS evaluation. JIT path uses inline compilation; AOT path spawns rts.",
    members: MEMBERS,
};
