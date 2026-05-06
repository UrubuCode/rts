use crate::abi::AbiType;
use super::{NodespaceMember, NodespaceSpec};

pub const MEMBERS: &[NodespaceMember] = &[
    NodespaceMember {
        name: "readFileSync",
        symbol: "__RTS_FN_NS_FS_READ_ALL",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "writeFileSync",
        symbol: "__RTS_FN_NS_FS_WRITE",
        args: &[AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::Void,
    },
    NodespaceMember {
        name: "appendFileSync",
        symbol: "__RTS_FN_NS_FS_APPEND",
        args: &[AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::Void,
    },
    NodespaceMember {
        name: "existsSync",
        symbol: "__RTS_FN_NS_FS_EXISTS",
        args: &[AbiType::StrPtr],
        returns: AbiType::Bool,
    },
    NodespaceMember {
        name: "mkdirSync",
        symbol: "__RTS_FN_NS_FS_CREATE_DIR_ALL",
        args: &[AbiType::StrPtr],
        returns: AbiType::Void,
    },
    NodespaceMember {
        name: "rmdirSync",
        symbol: "__RTS_FN_NS_FS_REMOVE_DIR",
        args: &[AbiType::StrPtr],
        returns: AbiType::Void,
    },
    NodespaceMember {
        name: "rmSync",
        symbol: "__RTS_FN_NS_FS_REMOVE_FILE",
        args: &[AbiType::StrPtr],
        returns: AbiType::Void,
    },
    NodespaceMember {
        name: "renameSync",
        symbol: "__RTS_FN_NS_FS_RENAME",
        args: &[AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::Void,
    },
    NodespaceMember {
        name: "copyFileSync",
        symbol: "__RTS_FN_NS_FS_COPY",
        args: &[AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::Void,
    },
    NodespaceMember {
        name: "readdirSync",
        symbol: "__RTS_FN_NS_FS_READDIR",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    // Stat helpers — node:fs.statSync retorna objeto Stats com varios
    // metodos. Como nodespace e' flat, expomos cada propriedade como
    // funcao top-level RTS-extension. O proximo PR pode adicionar
    // wrapper TS em builtin/ que constroe um objeto Stats real.
    NodespaceMember {
        name: "isFileSync",
        symbol: "__RTS_FN_NS_FS_IS_FILE",
        args: &[AbiType::StrPtr],
        returns: AbiType::Bool,
    },
    NodespaceMember {
        name: "isDirectorySync",
        symbol: "__RTS_FN_NS_FS_IS_DIR",
        args: &[AbiType::StrPtr],
        returns: AbiType::Bool,
    },
    NodespaceMember {
        name: "sizeSync",
        symbol: "__RTS_FN_NS_FS_SIZE",
        args: &[AbiType::StrPtr],
        returns: AbiType::I64,
    },
    NodespaceMember {
        name: "mtimeMsSync",
        symbol: "__RTS_FN_NS_FS_MODIFIED_MS",
        args: &[AbiType::StrPtr],
        returns: AbiType::I64,
    },
];

pub const SPEC: NodespaceSpec = NodespaceSpec {
    node_module: "fs",
    ns_prefix: "node_fs",
    members: MEMBERS,
};
