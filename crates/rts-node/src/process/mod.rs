//! `node:process` — the module-import surface of Node's `process` global.
//! Registration only; see `symbols.rs` for the implementations.

mod promises;
mod symbols;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        // None of these are pure: platform/arch/pid read process/OS state,
        // cwd/chdir touch process-global cwd, exit is a side-effecting
        // terminator. Mirrors the `os`/`env` namespace convention.
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `node:process` surface into the engine Registry. No
/// `.alias("process")`: an rts-std `process` NS namespace already owns the
/// bare `process` key with RTS-flavored semantics — scheme-aware routing
/// resolves `node:process` to this canonical `("node","process")` key
/// directly, with no collision.
pub fn register(e: &mut Engine) {
    e.ns("node:process")
        .doc(
            "Node process module surface (node:process): platform, arch, pid, \
             cwd, chdir, exit. argv/execArgv/env/versions/version/title/argv0 \
             (arrays/objects/properties), nextTick/hrtime (async/bigint), \
             stdout/stderr/stdin (streams), on(signal) (events) are deferred — \
             see the module doc comment for why.",
        )
        .member(func(
            "platform",
            "__RTS_FN_NODE_PROCESS_PLATFORM",
            sig!(=> Handle),
            "platform(): string",
            "Node's process.platform value ('win32', 'darwin', 'linux', ...).",
            symbols::__RTS_FN_NODE_PROCESS_PLATFORM as *const u8,
        ))
        .member(func(
            "arch",
            "__RTS_FN_NODE_PROCESS_ARCH",
            sig!(=> Handle),
            "arch(): string",
            "Node's process.arch value ('x64', 'ia32', 'arm64', ...).",
            symbols::__RTS_FN_NODE_PROCESS_ARCH as *const u8,
        ))
        .member(func(
            "pid",
            "__RTS_FN_NODE_PROCESS_PID",
            sig!(=> F64),
            "pid(): number",
            "The current process id.",
            symbols::__RTS_FN_NODE_PROCESS_PID as *const u8,
        ))
        .member(func(
            "cwd",
            "__RTS_FN_NODE_PROCESS_CWD",
            sig!(=> Handle),
            "cwd(): string",
            "The current working directory; empty string if it cannot be resolved.",
            symbols::__RTS_FN_NODE_PROCESS_CWD as *const u8,
        ))
        .member(func(
            "chdir",
            "__RTS_FN_NODE_PROCESS_CHDIR",
            sig!(StrPtr => Void),
            "chdir(dir: string): void",
            "Changes the current working directory (silently a no-op on failure — \
             no exception channel in this ABI shape).",
            symbols::__RTS_FN_NODE_PROCESS_CHDIR as *const u8,
        ))
        .member(func(
            "exit",
            "__RTS_FN_NODE_PROCESS_EXIT",
            sig!(I64 => Void),
            "exit(code: number): void",
            "Terminates the process immediately with the given exit code.",
            symbols::__RTS_FN_NODE_PROCESS_EXIT as *const u8,
        ))
        .done();
}
