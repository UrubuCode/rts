//! MessageChannel / MessagePort global-class specs. `new MessageChannel()`
//! resolves via `global_class_lookup`; `port1`/`port2` are read-only getters
//! returning typed `MessagePort` instances so `channel.port2.postMessage(x)`
//! dispatches as an InstanceMethod (a method call on an untyped getter-chain
//! receiver is not supported by codegen). `onmessage` is NOT a declared member
//! — the port is a plain object (Entry::Map), so `port.onmessage = cb` is a
//! generic property store the runtime later reads.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

const fn port_getter(
    name: &'static str,
    symbol: &'static str,
    ts: &'static str,
) -> NamespaceMember {
    NamespaceMember {
        name,
        kind: MemberKind::InstanceGetter,
        symbol,
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "MessageChannel port.",
        ts_signature: ts,
        intrinsic: None,
        pure: false,
    }
}

const fn method(
    name: &'static str,
    symbol: &'static str,
    args: &'static [AbiType],
    ts: &'static str,
) -> NamespaceMember {
    NamespaceMember {
        name,
        kind: MemberKind::InstanceMethod,
        symbol,
        args,
        returns: AbiType::Void,
        doc: "MessagePort instance method.",
        ts_signature: ts,
        intrinsic: None,
        pure: false,
    }
}

pub const MESSAGE_CHANNEL_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_MESSAGE_CHANNEL_NEW",
        args: &[],
        returns: AbiType::Handle,
        doc: "MessageChannel constructor — creates an entangled pair of ports.",
        ts_signature: "new MessageChannel(): MessageChannel",
        intrinsic: None,
        pure: false,
    },
    port_getter(
        "port1",
        "__RTS_FN_GL_MESSAGE_CHANNEL_PORT1",
        "readonly port1: MessagePort",
    ),
    port_getter(
        "port2",
        "__RTS_FN_GL_MESSAGE_CHANNEL_PORT2",
        "readonly port2: MessagePort",
    ),
];

pub const MESSAGE_CHANNEL_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "MessageChannel",
    doc: "MessageChannel — entangled MessagePort pair (synchronous delivery).",
    members: MESSAGE_CHANNEL_MEMBERS,
};

pub const MESSAGE_PORT_MEMBERS: &[NamespaceMember] = &[
    method(
        "postMessage",
        "__RTS_FN_GL_MESSAGE_PORT_POST_MESSAGE",
        &[AbiType::Handle, AbiType::Handle],
        "postMessage(data: any): void",
    ),
    method(
        "close",
        "__RTS_FN_GL_MESSAGE_PORT_CLOSE",
        &[AbiType::Handle],
        "close(): void",
    ),
];

pub const MESSAGE_PORT_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "MessagePort",
    doc: "MessagePort — one end of a MessageChannel.",
    members: MESSAGE_PORT_MEMBERS,
};
