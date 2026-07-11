//! `node:child_process` — the SYNCHRONOUS (blocking) subset over `std::process`:
//! `execSync(command)` (shell → stdout, throws on non-zero exit), `spawnSync(
//! command, args)` (→ a `{ pid, status, signal, stdout, stderr }` object),
//! `execFileSync(file, args)` (program + args → stdout). Real child processes.
//!
//! Deferred (need the event-loop / stream / EventEmitter-subclass subsystems):
//! the async `spawn`/`exec`/`execFile`/`fork` + the `ChildProcess` class
//! (its `stdin`/`stdout`/`stderr` streams + `'exit'`/`'close'`/`'message'`
//! events), IPC/`send`, and the full options objects (cwd/env/stdio/timeout/
//! maxBuffer/shell). Node returns Buffers for captured output; RTS returns UTF-8
//! strings.
//!
//! Layout: `exec` (operations + extern points), `mod` (registration).

mod exec;

use rts_engine::AbiType::{self, Handle, StrPtr};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn func(name: &str, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::THROWS,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `node:child_process` surface.
pub fn register(e: &mut Engine) {
    use exec as s;
    e.ns("node:child_process")
        .doc("Child processes (node:child_process): execSync, spawnSync, execFileSync (synchronous).")
        .member(func("execSync", vec![StrPtr], Handle, "__RTS_FN_NODE_CP_EXEC", "execSync(command: string): string", s::__RTS_FN_NODE_CP_EXEC as *const u8))
        .member(func("execFileSync", vec![StrPtr, Handle], Handle, "__RTS_FN_NODE_CP_EXEC_FILE", "execFileSync(file: string, args: string[]): string", s::__RTS_FN_NODE_CP_EXEC_FILE as *const u8))
        .member(func("spawnSync", vec![StrPtr, Handle], Handle, "__RTS_FN_NODE_CP_SPAWN", "spawnSync(command: string, args: string[]): object", s::__RTS_FN_NODE_CP_SPAWN as *const u8))
        .done();
}
