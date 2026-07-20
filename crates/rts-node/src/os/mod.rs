//! `node:os` — Registry registration for the full, Node-25-parity surface.
//!
//! Every member is a REAL syscall/algorithm (no stubs, no fixed values): the
//! identity strings, memory/uptime/load counters, CPU topology, network
//! interfaces, user identity, scheduling priority, and the per-platform
//! constants table. Objects/arrays are the genuine engine representation
//! (`words` builders over `alloc_shaped_object`/`Entry::Vec`), so they read
//! back from user JS as ordinary objects/arrays.
//!
//! Module layout: `words` (value-word/name helpers), `sys` (shared identity
//! syscalls), then one module per concern (`identity`, `meminfo`, `cpus`,
//! `netif`, `userinfo`, `priority`, `constants`).
//!
//! Deferred (needs an options-object/`.ts` shim layer rts-node does not ship
//! yet, honestly not faked): `os.userInfo({ encoding: 'buffer' })` (the string
//! fields as `Buffer`) — the default string form is fully real. Everything else
//! in the `node:os` surface is implemented.

mod constants;
mod cpus;
mod identity;
pub(crate) mod meminfo;
mod netif;
mod priority;
mod sys;
mod userinfo;
mod words;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

/// A pure (stable-per-process) function member — safe for the engine to cache.
fn pure_func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig, ts, fp, MemberKind::Function, true, MemberFlags::NONE)
}

/// An impure function member (time-varying scalar, or fresh-object/array
/// return) — must NOT be folded/deduped/cached by the engine.
fn func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig, ts, fp, MemberKind::Function, false, MemberFlags::NONE)
}

/// An impure function member that may THROW (pairs the pending-error slot with
/// the front's post-call unwind check).
fn throws_func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig, ts, fp, MemberKind::Function, false, MemberFlags::THROWS)
}

/// A `Constant` property getter (`os.EOL`/`os.devNull`/`os.constants`).
fn constant(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8) -> Member {
    make(name, symbol, sig, ts, fp, MemberKind::Constant, false, MemberFlags::NONE)
}

#[allow(clippy::too_many_arguments)]
fn make(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    fp: *const u8,
    kind: MemberKind,
    pure: bool,
    flags: MemberFlags,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure,
        intrinsic: None,
        emit: None,
    }
}

/// Registers the full `node:os` surface into the engine Registry. Its own
/// canonical key (no alias onto `rts:os`).
pub fn register(e: &mut Engine) {
    e.ns("node:os")
        .doc(
            "Real OS information, Node-accurate names/values (node:os): identity \
             strings, memory/uptime/loadavg, cpus()/networkInterfaces()/ \
             userInfo(), getPriority/setPriority, and EOL/devNull/constants.",
        )
        // --- identity strings ------------------------------------------------
        .member(pure_func(
            "platform",
            "__RTS_FN_NODE_OS_PLATFORM",
            sig!(=> Handle),
            "platform(): string",
            identity::__RTS_FN_NODE_OS_PLATFORM as *const u8,
        ))
        .member(pure_func(
            "arch",
            "__RTS_FN_NODE_OS_ARCH",
            sig!(=> Handle),
            "arch(): string",
            identity::__RTS_FN_NODE_OS_ARCH as *const u8,
        ))
        .member(pure_func(
            "type",
            "__RTS_FN_NODE_OS_TYPE",
            sig!(=> Handle),
            "type(): string",
            identity::__RTS_FN_NODE_OS_TYPE as *const u8,
        ))
        .member(pure_func(
            "endianness",
            "__RTS_FN_NODE_OS_ENDIANNESS",
            sig!(=> Handle),
            "endianness(): string",
            identity::__RTS_FN_NODE_OS_ENDIANNESS as *const u8,
        ))
        .member(pure_func(
            "machine",
            "__RTS_FN_NODE_OS_MACHINE",
            sig!(=> Handle),
            "machine(): string",
            identity::__RTS_FN_NODE_OS_MACHINE as *const u8,
        ))
        .member(pure_func(
            "release",
            "__RTS_FN_NODE_OS_RELEASE",
            sig!(=> Handle),
            "release(): string",
            identity::__RTS_FN_NODE_OS_RELEASE as *const u8,
        ))
        .member(pure_func(
            "version",
            "__RTS_FN_NODE_OS_VERSION",
            sig!(=> Handle),
            "version(): string",
            identity::__RTS_FN_NODE_OS_VERSION as *const u8,
        ))
        .member(func(
            "hostname",
            "__RTS_FN_NODE_OS_HOSTNAME",
            sig!(=> Handle),
            "hostname(): string",
            identity::__RTS_FN_NODE_OS_HOSTNAME as *const u8,
        ))
        .member(pure_func(
            "homedir",
            "__RTS_FN_NODE_OS_HOMEDIR",
            sig!(=> Handle),
            "homedir(): string",
            identity::__RTS_FN_NODE_OS_HOMEDIR as *const u8,
        ))
        .member(pure_func(
            "tmpdir",
            "__RTS_FN_NODE_OS_TMPDIR",
            sig!(=> Handle),
            "tmpdir(): string",
            identity::__RTS_FN_NODE_OS_TMPDIR as *const u8,
        ))
        // --- memory / uptime / load -----------------------------------------
        .member(func(
            "totalmem",
            "__RTS_FN_NODE_OS_TOTALMEM",
            sig!(=> F64),
            "totalmem(): number",
            meminfo::__RTS_FN_NODE_OS_TOTALMEM as *const u8,
        ))
        .member(func(
            "freemem",
            "__RTS_FN_NODE_OS_FREEMEM",
            sig!(=> F64),
            "freemem(): number",
            meminfo::__RTS_FN_NODE_OS_FREEMEM as *const u8,
        ))
        .member(func(
            "uptime",
            "__RTS_FN_NODE_OS_UPTIME",
            sig!(=> F64),
            "uptime(): number",
            meminfo::__RTS_FN_NODE_OS_UPTIME as *const u8,
        ))
        .member(func(
            "loadavg",
            "__RTS_FN_NODE_OS_LOADAVG",
            sig!(=> Handle),
            "loadavg(): number[]",
            meminfo::__RTS_FN_NODE_OS_LOADAVG as *const u8,
        ))
        .member(func(
            "availableParallelism",
            "__RTS_FN_NODE_OS_AVAILABLE_PARALLELISM",
            sig!(=> F64),
            "availableParallelism(): number",
            meminfo::__RTS_FN_NODE_OS_AVAILABLE_PARALLELISM as *const u8,
        ))
        // --- topology / interfaces / user -----------------------------------
        .member(func(
            "cpus",
            "__RTS_FN_NODE_OS_CPUS",
            sig!(=> Handle),
            "cpus(): object[]",
            cpus::__RTS_FN_NODE_OS_CPUS as *const u8,
        ))
        .member(func(
            "networkInterfaces",
            "__RTS_FN_NODE_OS_NETWORK_INTERFACES",
            sig!(=> Handle),
            "networkInterfaces(): object",
            netif::__RTS_FN_NODE_OS_NETWORK_INTERFACES as *const u8,
        ))
        .member(func(
            "userInfo",
            "__RTS_FN_NODE_OS_USER_INFO",
            sig!(=> Handle),
            "userInfo(): { username: string, uid: number, gid: number, shell: string | null, homedir: string }",
            userinfo::__RTS_FN_NODE_OS_USER_INFO as *const u8,
        ))
        // --- scheduling priority (optional pid → arity-overloaded members) ---
        .member(throws_func(
            "getPriority",
            "__RTS_FN_NODE_OS_GET_PRIORITY_SELF",
            sig!(=> I32),
            "getPriority(): number",
            priority::__RTS_FN_NODE_OS_GET_PRIORITY_SELF as *const u8,
        ))
        .member(throws_func(
            "getPriority",
            "__RTS_FN_NODE_OS_GET_PRIORITY",
            sig!(I32 => I32),
            "getPriority(pid: number): number",
            priority::__RTS_FN_NODE_OS_GET_PRIORITY as *const u8,
        ))
        .member(throws_func(
            "setPriority",
            "__RTS_FN_NODE_OS_SET_PRIORITY_SELF",
            sig!(I32 => Void),
            "setPriority(priority: number): void",
            priority::__RTS_FN_NODE_OS_SET_PRIORITY_SELF as *const u8,
        ))
        .member(throws_func(
            "setPriority",
            "__RTS_FN_NODE_OS_SET_PRIORITY",
            sig!(I32, I32 => Void),
            "setPriority(pid: number, priority: number): void",
            priority::__RTS_FN_NODE_OS_SET_PRIORITY as *const u8,
        ))
        // --- properties (Constant getters) ----------------------------------
        .member(constant(
            "EOL",
            "__RTS_FN_NODE_OS_EOL",
            sig!(=> Handle),
            "EOL: string",
            identity::__RTS_FN_NODE_OS_EOL as *const u8,
        ))
        .member(constant(
            "devNull",
            "__RTS_FN_NODE_OS_DEV_NULL",
            sig!(=> Handle),
            "devNull: string",
            identity::__RTS_FN_NODE_OS_DEV_NULL as *const u8,
        ))
        .member(constant(
            "constants",
            "__RTS_FN_NODE_OS_CONSTANTS",
            sig!(=> Handle),
            "constants: { signals: Record<string, number>, errno: Record<string, number>, dlopen: Record<string, number>, priority: Record<string, number>, libuv: { UV_UDP_REUSEADDR: number } }",
            constants::__RTS_FN_NODE_OS_CONSTANTS as *const u8,
        ))
        .done();
}
