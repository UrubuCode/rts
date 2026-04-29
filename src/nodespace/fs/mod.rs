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
];

pub const SPEC: NodespaceSpec = NodespaceSpec {
    node_module: "fs",
    ns_prefix: "node_fs",
    members: MEMBERS,
};
