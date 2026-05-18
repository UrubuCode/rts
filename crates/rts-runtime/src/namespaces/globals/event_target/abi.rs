use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

// ── EventTarget ───────────────────────────────────────────────────────────────

pub const EVENT_TARGET_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_EVENT_TARGET_NEW",
        args: &[],
        returns: AbiType::Handle,
        doc: "new EventTarget() — alvo de eventos.",
        ts_signature: "new EventTarget()",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "addEventListener",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EVENT_TARGET_ADD_LISTENER",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
        returns: AbiType::Void,
        doc: "target.addEventListener(type, fn)",
        ts_signature: "addEventListener(type: string, listener: (ev: Event) => void): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "removeEventListener",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EVENT_TARGET_REMOVE_LISTENER",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
        returns: AbiType::Void,
        doc: "target.removeEventListener(type, fn)",
        ts_signature: "removeEventListener(type: string, listener: (ev: Event) => void): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "dispatchEvent",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EVENT_TARGET_DISPATCH",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Bool,
        doc: "target.dispatchEvent(event) — chama listeners do event.type.",
        ts_signature: "dispatchEvent(event: Event): boolean",
        intrinsic: None,
        pure: false,
    },
];

pub const EVENT_TARGET_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "EventTarget",
    doc: "EventTarget — base de eventos sincronos.",
    members: EVENT_TARGET_MEMBERS,
};

// ── Event ─────────────────────────────────────────────────────────────────────

pub const EVENT_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_EVENT_NEW",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "new Event(type)",
        ts_signature: "new Event(type: string)",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "type",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_EVENT_TYPE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "event.type — string do nome do evento.",
        ts_signature: "readonly type: string",
        intrinsic: None,
        pure: true,
    },
];

pub const EVENT_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Event",
    doc: "Event — payload simples passado pra dispatchEvent.",
    members: EVENT_MEMBERS,
};
