use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

// ── AbortController ───────────────────────────────────────────────────────────

pub const ABORT_CONTROLLER_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_ABORT_CONTROLLER_NEW",
        args: &[],
        returns: AbiType::Handle,
        doc: "new AbortController() — cria controller com signal vazio.",
        ts_signature: "new AbortController()",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "signal",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_ABORT_CONTROLLER_SIGNAL",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "controller.signal — AbortSignal associado.",
        ts_signature: "readonly signal: AbortSignal",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "abort",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ABORT_CONTROLLER_ABORT",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "controller.abort(reason?) — aborta signal, dispara listeners.",
        ts_signature: "abort(reason?: any): void",
        intrinsic: None,
        pure: false,
    },
];

pub const ABORT_CONTROLLER_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "AbortController",
    doc: "AbortController — sinal abortavel para cancelar operacoes.",
    members: ABORT_CONTROLLER_MEMBERS,
};

// ── AbortSignal ───────────────────────────────────────────────────────────────

pub const ABORT_SIGNAL_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "aborted",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_ABORT_SIGNAL_ABORTED",
        args: &[AbiType::Handle],
        returns: AbiType::Bool,
        doc: "signal.aborted — true se ja' foi abortado.",
        ts_signature: "readonly aborted: boolean",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "reason",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_ABORT_SIGNAL_REASON",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "signal.reason — handle do reason passado em abort().",
        ts_signature: "readonly reason: any",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "addEventListener",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ABORT_SIGNAL_ADD_LISTENER",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
        returns: AbiType::Void,
        doc: "signal.addEventListener(type, fn) — so type='abort' efetivo.",
        ts_signature: "addEventListener(type: string, listener: () => void): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "removeEventListener",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ABORT_SIGNAL_REMOVE_LISTENER",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
        returns: AbiType::Void,
        doc: "signal.removeEventListener(type, fn)",
        ts_signature: "removeEventListener(type: string, listener: () => void): void",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "throwIfAborted",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_ABORT_SIGNAL_THROW_IF_ABORTED",
        args: &[AbiType::Handle],
        returns: AbiType::Void,
        doc: "signal.throwIfAborted() — set runtime error se aborted.",
        ts_signature: "throwIfAborted(): void",
        intrinsic: None,
        pure: false,
    },
    // ── Static ───────────────────────────────────────────────────────────────
    NamespaceMember {
        name: "abort",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_ABORT_SIGNAL_STATIC_ABORT",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "AbortSignal.abort(reason?) — cria signal ja' aborted.",
        ts_signature: "static abort(reason?: any): AbortSignal",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "timeout",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_ABORT_SIGNAL_TIMEOUT",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
        doc: "AbortSignal.timeout(ms) — aborta apos ms.",
        ts_signature: "static timeout(ms: number): AbortSignal",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "any",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_ABORT_SIGNAL_ANY",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "AbortSignal.any(signals) — aborta quando qualquer signal abortar.",
        ts_signature: "static any(signals: AbortSignal[]): AbortSignal",
        intrinsic: None,
        pure: false,
    },
];

pub const ABORT_SIGNAL_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "AbortSignal",
    doc: "AbortSignal — sinal abortavel observavel.",
    members: ABORT_SIGNAL_MEMBERS,
};
