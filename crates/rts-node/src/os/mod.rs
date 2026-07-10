//! `node:os` — Registry registration (member table + `register`).

mod promises;
mod symbols;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:os` surface into the engine Registry. No `.alias(...)`
/// — `node:os` resolves as its own canonical key, distinct from the existing
/// `rts:os` namespace (RTS-flavored names/semantics), so it must not alias
/// onto it.
pub fn register(e: &mut Engine) {
    e.ns("node:os")
        .doc(
            "Real system info, Node-accurate names/values (node:os). EOL/devNull/ \
             constants (properties), cpus()/networkInterfaces()/userInfo() \
             (objects/arrays), and hostname()/release()/version()/totalmem()/ \
             freemem()/uptime()/loadavg() (need syscalls beyond std) are deferred.",
        )
        .member(pure_func(
            "platform",
            "__RTS_FN_NODE_OS_PLATFORM",
            sig!(=> Handle),
            "platform(): string",
            "Canonical Node platform name: 'win32', 'darwin', 'linux', 'freebsd', ...",
            symbols::__RTS_FN_NODE_OS_PLATFORM as *const u8,
        ))
        .member(pure_func(
            "arch",
            "__RTS_FN_NODE_OS_ARCH",
            sig!(=> Handle),
            "arch(): string",
            "Canonical Node CPU architecture name: 'x64', 'arm64', 'ia32', ...",
            symbols::__RTS_FN_NODE_OS_ARCH as *const u8,
        ))
        .member(pure_func(
            "type",
            "__RTS_FN_NODE_OS_TYPE",
            sig!(=> Handle),
            "type(): string",
            "Kernel name Node uses: 'Windows_NT', 'Darwin', 'Linux', ...",
            symbols::__RTS_FN_NODE_OS_TYPE as *const u8,
        ))
        .member(pure_func(
            "endianness",
            "__RTS_FN_NODE_OS_ENDIANNESS",
            sig!(=> Handle),
            "endianness(): string",
            "Native byte order of the target: 'LE' or 'BE'.",
            symbols::__RTS_FN_NODE_OS_ENDIANNESS as *const u8,
        ))
        .member(pure_func(
            "homedir",
            "__RTS_FN_NODE_OS_HOMEDIR",
            sig!(=> Handle),
            "homedir(): string",
            "Real user home directory. Empty string if unresolvable.",
            symbols::__RTS_FN_NODE_OS_HOMEDIR as *const u8,
        ))
        .member(pure_func(
            "tmpdir",
            "__RTS_FN_NODE_OS_TMPDIR",
            sig!(=> Handle),
            "tmpdir(): string",
            "System temporary directory, no trailing path separator.",
            symbols::__RTS_FN_NODE_OS_TMPDIR as *const u8,
        ))
        .member(pure_func(
            "availableParallelism",
            "__RTS_FN_NODE_OS_AVAILABLE_PARALLELISM",
            sig!(=> F64),
            "availableParallelism(): number",
            "Logical CPUs available to this process (fallback 1 on query error).",
            symbols::__RTS_FN_NODE_OS_AVAILABLE_PARALLELISM as *const u8,
        ))
        .done();
}
