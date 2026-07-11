//! `node:process` — the clean, synchronous flat surface of Node's process
//! object: `cwd`/`chdir`, `platform`/`arch`/`pid`, `exit`/`abort`, `uptime`/
//! `hrtime`, `version`/`versions`, `argv`/`argv0`/`execPath`/`title`,
//! `getActiveResourcesInfo`, and an `env` snapshot. Every value is read live
//! from the OS (`std::env`/`std::process`/`std::time`) — no hardcoded data.
//!
//! `nextTick` maps onto the engine microtask queue (drained at program end;
//! forwarded `...args` deferred). Deferred (need subsystems RTS does not expose
//! here yet): `stdout`/`stderr`/`stdin` (the stream layer), `on`/`emit`
//! + signal handling (process is an EventEmitter singleton, not a `new`-able
//! class), `env` write-through (`process.env.X = v` → `setenv`, needs an object
//! write-proxy), `hrtime.bigint` (BigInt return), `resourceUsage` (the full
//! rusage struct), `kill`.
//!
//! Layout: `words` (value helpers + platform/arch map + clock base), `symbols`
//! (extern points), `mod` (registration).

mod metrics;
mod symbols;
mod words;

use rts_engine::AbiType::{self, F64, Handle, I64, StrPtr, U64, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

// The engine's microtask-queue enqueue (drained at end of the program) — backs
// `process.nextTick`. Declared here so its real address becomes the member's
// fn_ptr (JIT-harvested), the same reuse pattern node:timers uses.
unsafe extern "C" {
    fn __RTS_FN_GL_TEXTENC_QUEUE_MICROTASK(fp: u64);
}

fn f(name: &str, symbol: &str, args: Vec<AbiType>, ret: AbiType, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `node:process` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:process")
        .doc("Process information (node:process): cwd/chdir, platform/arch/pid, exit/abort, uptime/hrtime, version/versions, argv/execPath/title, env.")
        .member(f("cwd", "__RTS_FN_NODE_PROC_CWD", vec![], Handle, "cwd(): string", s::__RTS_FN_NODE_PROC_CWD as *const u8))
        .member(f("chdir", "__RTS_FN_NODE_PROC_CHDIR", vec![StrPtr], Void, "chdir(directory: string): void", s::__RTS_FN_NODE_PROC_CHDIR as *const u8))
        .member(f("platform", "__RTS_FN_NODE_PROC_PLATFORM", vec![], Handle, "platform(): string", s::__RTS_FN_NODE_PROC_PLATFORM as *const u8))
        .member(f("arch", "__RTS_FN_NODE_PROC_ARCH", vec![], Handle, "arch(): string", s::__RTS_FN_NODE_PROC_ARCH as *const u8))
        .member(f("pid", "__RTS_FN_NODE_PROC_PID", vec![], I64, "pid(): number", s::__RTS_FN_NODE_PROC_PID as *const u8))
        .member(f("exit", "__RTS_FN_NODE_PROC_EXIT", vec![I64], Void, "exit(code?: number): void", s::__RTS_FN_NODE_PROC_EXIT as *const u8))
        .member(f("abort", "__RTS_FN_NODE_PROC_ABORT", vec![], Void, "abort(): void", s::__RTS_FN_NODE_PROC_ABORT as *const u8))
        .member(f("uptime", "__RTS_FN_NODE_PROC_UPTIME", vec![], F64, "uptime(): number", s::__RTS_FN_NODE_PROC_UPTIME as *const u8))
        .member(f("hrtime", "__RTS_FN_NODE_PROC_HRTIME", vec![], Handle, "hrtime(): number[]", s::__RTS_FN_NODE_PROC_HRTIME as *const u8))
        .member(f("hrtime", "__RTS_FN_NODE_PROC_HRTIME_DIFF", vec![Handle], Handle, "hrtime(prev: number[]): number[]", s::__RTS_FN_NODE_PROC_HRTIME_DIFF as *const u8))
        .member(f("version", "__RTS_FN_NODE_PROC_VERSION", vec![], Handle, "version(): string", s::__RTS_FN_NODE_PROC_VERSION as *const u8))
        .member(f("versions", "__RTS_FN_NODE_PROC_VERSIONS", vec![], Handle, "versions(): object", s::__RTS_FN_NODE_PROC_VERSIONS as *const u8))
        .member(f("argv", "__RTS_FN_NODE_PROC_ARGV", vec![], Handle, "argv(): string[]", s::__RTS_FN_NODE_PROC_ARGV as *const u8))
        .member(f("argv0", "__RTS_FN_NODE_PROC_ARGV0", vec![], Handle, "argv0(): string", s::__RTS_FN_NODE_PROC_ARGV0 as *const u8))
        .member(f("execPath", "__RTS_FN_NODE_PROC_EXEC_PATH", vec![], Handle, "execPath(): string", s::__RTS_FN_NODE_PROC_EXEC_PATH as *const u8))
        .member(f("title", "__RTS_FN_NODE_PROC_TITLE", vec![], Handle, "title(): string", s::__RTS_FN_NODE_PROC_TITLE as *const u8))
        .member(f("getActiveResourcesInfo", "__RTS_FN_NODE_PROC_ACTIVE_RESOURCES", vec![], Handle, "getActiveResourcesInfo(): string[]", s::__RTS_FN_NODE_PROC_ACTIVE_RESOURCES as *const u8))
        .member(f("env", "__RTS_FN_NODE_PROC_ENV", vec![], Handle, "env(): object", s::__RTS_FN_NODE_PROC_ENV as *const u8))
        .member(f("memoryUsage", "__RTS_FN_NODE_PROC_MEMORY_USAGE", vec![], Handle, "memoryUsage(): object", metrics::__RTS_FN_NODE_PROC_MEMORY_USAGE as *const u8))
        .member(f("cpuUsage", "__RTS_FN_NODE_PROC_CPU_USAGE", vec![], Handle, "cpuUsage(): object", metrics::__RTS_FN_NODE_PROC_CPU_USAGE as *const u8))
        .member(f("availableMemory", "__RTS_FN_NODE_PROC_AVAILABLE_MEMORY", vec![], F64, "availableMemory(): number", metrics::__RTS_FN_NODE_PROC_AVAILABLE_MEMORY as *const u8))
        .member(f("constrainedMemory", "__RTS_FN_NODE_PROC_CONSTRAINED_MEMORY", vec![], F64, "constrainedMemory(): number", metrics::__RTS_FN_NODE_PROC_CONSTRAINED_MEMORY as *const u8))
        // nextTick maps onto the engine's microtask queue (drained at program
        // end). The forwarded `...args` are deferred (the queue invokes the
        // callback with no arguments).
        .member(f("nextTick", "__RTS_FN_GL_TEXTENC_QUEUE_MICROTASK", vec![U64], Void, "nextTick(callback: () => void): void", __RTS_FN_GL_TEXTENC_QUEUE_MICROTASK as *const u8))
        .done();
}
