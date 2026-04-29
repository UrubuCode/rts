use crate::abi::AbiType;
use super::{NodespaceMember, NodespaceSpec};

pub const MEMBERS: &[NodespaceMember] = &[
    NodespaceMember {
        name: "platform",
        symbol: "__RTS_FN_NS_OS_PLATFORM",
        args: &[],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "arch",
        symbol: "__RTS_FN_NS_OS_ARCH",
        args: &[],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "homedir",
        symbol: "__RTS_FN_NS_OS_HOME_DIR",
        args: &[],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "tmpdir",
        symbol: "__RTS_FN_NS_OS_TEMP_DIR",
        args: &[],
        returns: AbiType::Handle,
    },
];

pub const SPEC: NodespaceSpec = NodespaceSpec {
    node_module: "os",
    ns_prefix: "node_os",
    members: MEMBERS,
};
