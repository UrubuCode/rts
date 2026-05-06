use crate::abi::AbiType;
use super::{NodespaceMember, NodespaceSpec};

pub const MEMBERS: &[NodespaceMember] = &[
    NodespaceMember {
        name: "exit",
        symbol: "__RTS_FN_NS_PROCESS_EXIT",
        args: &[AbiType::I64],
        returns: AbiType::Void,
    },
    NodespaceMember {
        name: "pid",
        symbol: "__RTS_FN_NS_PROCESS_PID",
        args: &[],
        returns: AbiType::I64,
    },
    NodespaceMember {
        name: "cwd",
        symbol: "__RTS_FN_NS_ENV_CWD",
        args: &[],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "chdir",
        symbol: "__RTS_FN_NS_ENV_SET_CWD",
        args: &[AbiType::StrPtr],
        returns: AbiType::Void,
    },
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
];

pub const SPEC: NodespaceSpec = NodespaceSpec {
    node_module: "process",
    ns_prefix: "node_process",
    members: MEMBERS,
};
