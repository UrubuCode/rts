use crate::abi::AbiType;
use super::{NodespaceMember, NodespaceSpec};

pub const MEMBERS: &[NodespaceMember] = &[
    NodespaceMember {
        name: "join",
        symbol: "__RTS_FN_NS_PATH_JOIN",
        args: &[AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "dirname",
        symbol: "__RTS_FN_NS_PATH_PARENT",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "basename",
        symbol: "__RTS_FN_NS_PATH_FILE_NAME",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "extname",
        symbol: "__RTS_FN_NS_PATH_EXT",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "isAbsolute",
        symbol: "__RTS_FN_NS_PATH_IS_ABSOLUTE",
        args: &[AbiType::StrPtr],
        returns: AbiType::Bool,
    },
    NodespaceMember {
        name: "normalize",
        symbol: "__RTS_FN_NS_PATH_NORMALIZE",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "resolve",
        symbol: "__RTS_FN_NS_PATH_NORMALIZE",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
];

pub const SPEC: NodespaceSpec = NodespaceSpec {
    node_module: "path",
    ns_prefix: "node_path",
    members: MEMBERS,
};
