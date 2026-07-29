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
        ret_class: None,
        emit: None,
    }
}

/// Registers the full `node:os` surface into the engine Registry. Its own
/// canonical key (no alias onto `rts:os`).
pub fn register(e: &mut Engine) {
    e.module("node:os", |m| {
        m.doc(
            "Real OS information, Node-accurate names/values (node:os): identity \
             strings, memory/uptime/loadavg, cpus()/networkInterfaces()/ \
             userInfo(), getPriority/setPriority, and EOL/devNull/constants.",
        )
        // --- identity strings ------------------------------------------------
        .registry(identity::platform_entry())
        .registry(identity::arch_entry())
        .registry(identity::os_type_entry())
        .registry(identity::endianness_entry())
        .registry(identity::machine_entry())
        .registry(identity::release_entry())
        .registry(identity::version_entry())
        .registry(identity::hostname_entry())
        .registry(identity::homedir_entry())
        .registry(identity::tmpdir_entry())
        // --- memory / uptime / load -----------------------------------------
        .registry(meminfo::totalmem_entry())
        .registry(meminfo::freemem_entry())
        .registry(meminfo::uptime_entry())
        .registry(meminfo::loadavg_entry())
        .registry(meminfo::available_parallelism_fn_entry())
        // --- topology / interfaces / user -----------------------------------
        .registry(cpus::cpus_entry())
        .registry(netif::network_interfaces_entry())
        .registry(userinfo::user_info_entry())
        // --- scheduling priority (optional pid → arity-overloaded members) ---
        .registry(priority::get_priority_self_entry())
        .registry(priority::get_priority_pid_entry())
        .registry(priority::set_priority_self_entry())
        .registry(priority::set_priority_pid_entry())
        // --- properties (Constant getters) ----------------------------------
        .registry(identity::eol_entry())
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
        ));
    });
}
