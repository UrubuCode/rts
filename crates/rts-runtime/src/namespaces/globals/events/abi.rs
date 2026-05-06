//! `EventEmitter` global class — ABI registration as `GlobalClassSpec`.
//!
//! Hybrid sync/async event dispatch:
//!   new EventEmitter()     — synchronous: listeners called in-order on the caller thread
//!   new EventEmitter(true) — async: each listener dispatched on rayon thread pool (fire-and-forget)
//!
//! Listener signature: `extern "C" fn(i64) -> i64` (single i64 arg, return ignored).
//! Arg is any i64/handle — caller decides the contract per-event.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

pub const MEMBERS: &[NamespaceMember] = &[
    // ── Destructor ────────────────────────────────────────────────────────────
    NamespaceMember {
        name: "free",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_FREE",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "Frees the EventEmitter handle. Returns 1 on success, 0 if already invalid.",
        ts_signature: "free(): number",
        intrinsic: None,
        pure: false,
    },
    // ── Constructors ──────────────────────────────────────────────────────────
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_EE_NEW",
        args: &[],
        returns: AbiType::Handle,
        doc: "Creates a synchronous EventEmitter.",
        ts_signature: "new EventEmitter(): EventEmitter",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "new_async",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_EE_NEW_ASYNC",
        args: &[AbiType::Bool],
        returns: AbiType::Handle,
        doc: "Creates an EventEmitter. Pass true for async (rayon) dispatch.",
        ts_signature: "new EventEmitter(async: boolean): EventEmitter",
        intrinsic: None,
        pure: false,
    },
    // ── Instance methods ──────────────────────────────────────────────────────
    NamespaceMember {
        name: "on",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_ON",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::U64],
        returns: AbiType::Handle,
        doc: "Registers a persistent listener for the named event. Returns `this` for chaining.",
        ts_signature: "on(event: string, listener: (arg: number) => void): this",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "once",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_ONCE",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::U64],
        returns: AbiType::Handle,
        doc: "Registers a one-shot listener that auto-removes after first call.",
        ts_signature: "once(event: string, listener: (arg: number) => void): this",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "off",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_OFF",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::U64],
        returns: AbiType::Handle,
        doc: "Removes a specific listener. Returns `this` for chaining.",
        ts_signature: "off(event: string, listener: (arg: number) => void): this",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "emit",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_EMIT",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::I64],
        returns: AbiType::Bool,
        doc: "Emits an event with a numeric payload (i64 → f64 numeric conversion). For handles use emitHandle().",
        ts_signature: "emit(event: string, arg: number): boolean",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "emitHandle",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_EMIT_HANDLE",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::I64],
        returns: AbiType::Bool,
        doc: "Emits an event with a handle/raw-i64 payload. Bitcasts bits to f64 so all 64 bits are preserved. Listener recovers the handle via num.f64_to_bits(arg).",
        ts_signature: "emitHandle(event: string, handle: number): boolean",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "removeAllListeners",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_REMOVE_ALL",
        args: &[AbiType::Handle, AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Removes all listeners for the named event. Returns `this`.",
        ts_signature: "removeAllListeners(event: string): this",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "listenerCount",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_LISTENER_COUNT",
        args: &[AbiType::Handle, AbiType::StrPtr],
        returns: AbiType::I64,
        doc: "Returns the number of listeners registered for the named event.",
        ts_signature: "listenerCount(event: string): number",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "eventNames",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_EE_EVENT_NAMES",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Returns a Vec handle containing the event name string handles.",
        ts_signature: "eventNames(): string[]",
        intrinsic: None,
        pure: true,
    },
];

pub const CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "EventEmitter",
    doc: "Node.js-compatible EventEmitter. Sync by default; pass `true` for rayon async dispatch.",
    members: MEMBERS,
};
