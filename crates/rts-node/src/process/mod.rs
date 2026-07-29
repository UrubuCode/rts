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
//! rusage struct).
//!
//! Layout: `words` (value helpers + platform/arch map + clock base), `symbols`
//! (extern points), `mod` (registration).

mod metrics;
mod signal;
mod symbols;
mod words;

use rts_engine::AbiType::{U64, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

// The engine's microtask-queue enqueue (drained at end of the program) — backs
// `process.nextTick`. Declared here so its real address becomes the member's
// fn_ptr (JIT-harvested), the same reuse pattern node:timers uses.
unsafe extern "C" {
    fn __RTS_FN_GL_TEXTENC_QUEUE_MICROTASK(fp: u64);
}

fn f(name: &str, symbol: &str, args: Vec<rts_engine::AbiType>, ret: rts_engine::AbiType, ts: &str, fp: *const u8) -> Member {
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
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registers the `node:process` surface.
pub fn register(e: &mut Engine) {
    e.module("node:process", |m| {
        m.doc("Process information (node:process): cwd/chdir, platform/arch/pid, exit/abort, uptime/hrtime, version/versions, argv/execPath/title, env.");
        m.registry(symbols::cwd_entry());
        m.registry(symbols::chdir_entry());
        m.registry(symbols::platform_entry());
        m.registry(symbols::arch_entry());
        m.registry(symbols::pid_entry());
        m.registry(symbols::exit_entry());
        m.registry(symbols::abort_entry());
        m.registry(symbols::uptime_entry());
        m.registry(symbols::hrtime_entry());
        m.registry(symbols::hrtime_diff_entry());
        m.registry(symbols::version_entry());
        m.registry(symbols::versions_entry());
        m.registry(symbols::argv_entry());
        m.registry(symbols::argv0_entry());
        m.registry(symbols::exec_path_entry());
        m.registry(symbols::title_entry());
        m.registry(symbols::active_resources_entry());
        m.registry(symbols::env_entry());
        m.registry(metrics::memory_usage_entry());
        m.registry(metrics::cpu_usage_entry());
        m.registry(metrics::available_memory_entry());
        m.registry(metrics::constrained_memory_entry());
        m.registry(metrics::resource_usage_entry());
        // nextTick maps onto the engine's microtask queue (drained at program
        // end). The forwarded `...args` are deferred (the queue invokes the
        // callback with no arguments). BLOCKED from `#[rtse::function]`: the
        // symbol is `__RTS_FN_GL_TEXTENC_QUEUE_MICROTASK`, owned/defined by the
        // `node:timers`/global TextEncoder module, not a fn in this module —
        // the macro always derives+defines the extern fn body itself, so it
        // cannot bind to a symbol that already exists elsewhere.
        m.member(f("nextTick", "__RTS_FN_GL_TEXTENC_QUEUE_MICROTASK", vec![U64], Void, "nextTick(callback: () => void): void", __RTS_FN_GL_TEXTENC_QUEUE_MICROTASK as *const u8));
        m.registry(signal::kill_entry());
    });
}
