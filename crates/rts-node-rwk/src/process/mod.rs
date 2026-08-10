//! `node:process` — the running program, as an object.
//!
//! # Reuse-check (mandatory, per `.claude/skills/reuse-check`)
//!
//! Searched before writing, and what each search answered:
//!
//! - *"a queue drained after the program's last statement"* —
//!   [`rts_core_rwk::entry::declare_loop_source`] / `Pending`, already the ONE
//!   answer six modules had each grown privately. `nextTick` is registered as a
//!   loop source rather than growing a seventh copy, and
//!   [`crate::timers`]'s `TIMERS`/`pump`/`source` shape is followed exactly:
//!   thread-local table, borrow released before a callback runs.
//! - *"an event emitter"* — [`crate::events`] owns the one `EventEmitter`
//!   prototype, made once by name. `process` is linked to **that** object
//!   rather than growing its own `on`/`emit`, so `process.on` IS
//!   `EventEmitter.prototype.on`, and the listener storage
//!   (`__events__`) is the one that module reads.
//! - *"Node's `(x[, options], callback)` shift"* — `crate::fs`'s
//!   `options_and_listener`. Nothing in this module has that shape
//!   (`emitWarning`'s second argument is options-or-string, never a callback),
//!   so it is not called; it is named here so the next reader does not search
//!   again.
//! - *"a bigint from Rust"* — [`rts_core_rwk::entry::make_bigint`], which
//!   exists now and did not when this module's predecessor refused
//!   `hrtime.bigint` for its absence. That refusal is withdrawn and the member
//!   is implemented.
//! - *"process memory / cpu accounting"* — `memoryUsage`, `cpuUsage`,
//!   `resourceUsage` and `kill` are now real, cross-platform (POSIX
//!   `getrusage`/`kill`, Windows `GetProcessTimes`/`K32GetProcessMemoryInfo`/
//!   `OpenProcess`+`TerminateProcess`, raw `extern "system"` declarations,
//!   `clock.rs`/`lifecycle.rs`). What is still absent is the V8-heap
//!   breakdown `memoryUsage` also reports (`heapTotal`/`heapUsed`/`external`/
//!   `arrayBuffers`): nothing on the host surface tracks a separate JS-heap
//!   byte count, so those fields are `0` rather than fabricated.
//!
//! # Property vs function
//!
//! `platform`, `arch`, `version`, `versions`, `pid`, `argv`, `argv0`,
//! `execPath`, `env` are PROPERTIES here, not functions — `process.platform`,
//! never `process.platform()`. That is the opposite convention from
//! [`crate::os`]'s `os.platform()`, and the two are deliberately not unified:
//! Node itself disagrees between the two modules, and a program written
//! against Node reads `process.platform` as a value.
//!
//! # Read once
//!
//! `argv`, `argv0`, `execPath`, `execArgv`, `env`, `platform`, `arch`,
//! `version`, `versions`, `release`, `features`, `pid`, `ppid`, `title` are all
//! read ONCE, at namespace construction, and become fixed values on the
//! namespace object. `cwd()`, `uptime()`, `hrtime()`, `cpuUsage()` are the
//! members that must NOT be captured that way — a program can `chdir`, and
//! elapsed time is elapsed by definition, so answering either at startup under
//! the name of a function would be a function that lies about being one.
//!
//! # `process` is an `EventEmitter`, and which events actually arrive
//!
//! The object is linked to `node:events`' own `EventEmitter.prototype`, so
//! `on`, `once`, `off`, `emit`, `listeners`, `removeAllListeners` and the rest
//! are the real implementations, and `process.emit('x')` reaches a listener
//! `process.on('x')` registered. What a program should NOT expect is that the
//! engine fires Node's lifecycle events: only **`'exit'`** (from
//! [`lifecycle::exit`]) and **`'warning'`** (from [`lifecycle::emit_warning`])
//! are emitted by this module. Every other documented event is refused by name
//! below, because delivering none of them silently while accepting the
//! registration is exactly the "registered and never run" shape this crate
//! treats as worse than absent.
//!
//! # `version` / `versions` — a named divergence
//!
//! There is no Node version behind this engine — this is not Node. `version`
//! and `versions.node` both answer this crate's own Cargo package version,
//! prefixed `v` the way Node's is, so a program's `console.log(process.version)`
//! prints something version-shaped rather than nothing. A program that
//! branches on a specific Node version will read the wrong one; that is the
//! divergence, named here rather than met with a fabricated Node number.
//!
//! # Not implemented, by name
//!
//! **Needs a host capability that does not exist** (reported, not worked
//! around): `threadCpuUsage`, `availableMemory`, `constrainedMemory` — the
//! GC-heap fields of `memoryUsage` (see the reuse-check note above for what
//! IS real now). `exitCode` is settable and IS honoured by [`lifecycle::exit`], but a program
//! that sets it and simply *ends* exits 0 — the host has no
//! end-of-program hook a module can ask to consult it.
//! `'beforeExit'`, `'uncaughtException'`, `'uncaughtExceptionMonitor'`,
//! `'unhandledRejection'`, `'rejectionHandled'`, `'worker'`,
//! `'workerMessage'` and every signal event: each needs an engine dispatch
//! point (empty-loop detection, top-level throw, promise settle-tracking, an OS
//! signal bridge) that no crate `rts-node-rwk` can depend on provides.
//! `setUncaughtExceptionCaptureCallback`, `addUncaughtExceptionCaptureCallback`,
//! `hasUncaughtExceptionCaptureCallback` — same missing dispatch point.
//! `getActiveResourcesInfo` — no handle-table enumeration is exported.
//! `setSourceMapsEnabled`/`sourceMapsEnabled` — a flag nothing reads is a
//! silent no-op, so it is absent rather than stored.
//!
//! **Refused as a wrong answer waiting to happen**: assigning to
//! `process.title` changes the property and NOT the OS process title (no `std`
//! equivalent, and no dependency may be added from here), so the read is
//! provided and the write is documented as local-only.
//! `process.env` is a SNAPSHOT: writes are visible to the program and are not
//! propagated to the OS environment block, so a child process spawned later
//! does not inherit them; `delete process.env.X` likewise removes only the
//! property. Windows case-insensitive `env` keys are not folded.
//!
//! **Absent because the mechanism is absent**: `send`, `disconnect`,
//! `channel`, `connected` and the `'message'`/`'disconnect'` events (no IPC
//! transport); `report.*` and `permission.*` and `finalization.*`; `ref`,
//! `unref`; `dlopen` (no addon loader in this engine); `mainModule`;
//! `config`; `debugPort`; `allowedNodeEnvironmentFlags` (needs a `Set`
//! subclass this crate cannot build); `execve`, `setuid`/`seteuid`/`setgid`/
//! `setegid`/`getgroups`/`setgroups`/`initgroups`; `noDeprecation`/
//! `throwDeprecation`/`traceDeprecation`/`traceProcessWarnings` (nothing
//! consults them, and `emitWarning` honouring flags nobody else does would be
//! half a mechanism).
//!
//! **POSIX-only, absent on Windows rather than fabricated**: `ppid`,
//! `umask`, `getuid`, `geteuid`, `getgid`, `getegid`. Each is a `libc` call
//! with no Windows equivalent reachable without a new dependency, and Node
//! itself refuses most of them there. (`kill` and `cpuUsage` used to be on
//! this list; both are cross-platform now — see the reuse-check note.)

mod clock;
mod info;
mod lifecycle;
mod streams;

use rts_core_rwk::entry::{Context, Provided};
use std::cell::Cell;

thread_local! {
    /// This thread's `process` object.
    ///
    /// Remembered because a native that wants to fire an event has to reach the
    /// emitter, and an `extern "C"` frame receives only its arguments. Per
    /// thread rather than in a `static` for the reason [`crate::timers`]'s
    /// table is: the value names a cell in the region of the thread that made
    /// it, and a worker's `process` is its own object.
    static PROCESS: Cell<u64> = const { Cell::new(0) };
}

/// The namespace `node:process` is.
pub fn namespace(context: &mut Context) -> u64 {
    // `mut` only on POSIX, where the block below extends it. Written as an
    // allow rather than two `cfg`-selected lists, which would be two places to
    // add a member to and one of them would drift.
    #[allow(unused_mut)]
    let mut members: Vec<(&str, Provided)> = vec![
        ("cwd", lifecycle::cwd),
        ("chdir", lifecycle::chdir),
        ("exit", lifecycle::exit),
        ("abort", lifecycle::abort),
        ("nextTick", lifecycle::next_tick),
        ("emitWarning", lifecycle::emit_warning),
        ("uptime", clock::uptime),
        ("getBuiltinModule", info::get_builtin_module),
        ("loadEnvFile", info::load_env_file),
        // Cross-platform (POSIX `getrusage`/`kill`, Windows `GetProcessTimes`/
        // `K32GetProcessMemoryInfo`/`OpenProcess`+`TerminateProcess` — see
        // `clock.rs` and `lifecycle.rs::kill`'s Windows forms).
        ("kill", lifecycle::kill as Provided),
        ("cpuUsage", clock::cpu_usage),
        ("memoryUsage", clock::memory_usage),
        ("resourceUsage", clock::resource_usage),
        ("availableMemory", clock::available_memory),
        ("constrainedMemory", clock::constrained_memory),
    ];
    // Absent, not stubbed: see the module doc's POSIX paragraph. A member that
    // exists and answers nothing reads as "implemented and broken"; an absent
    // one reads as what it is.
    #[cfg(unix)]
    members.extend_from_slice(&[
        ("umask", lifecycle::umask),
        ("getuid", lifecycle::getuid),
        ("geteuid", lifecycle::geteuid),
        ("getgid", lifecycle::getgid),
        ("getegid", lifecycle::getegid),
    ]);
    let namespace = rts_core_rwk::entry::make_namespace(context, &members);
    PROCESS.with(|held| held.set(namespace));

    info::install(context, namespace);
    clock::install(context, namespace);
    streams::install(context, namespace);

    // The emitter surface, taken from `node:events` rather than re-derived.
    //
    // NOT `make_prototype(context, "EventEmitter", &[])`: that call REGISTERS
    // whatever it builds under the name, so if this module ever ran first it
    // would register an empty prototype and `node:events` would then find it
    // and never install its methods — every emitter in the program silently
    // without `on`. `lib.rs` orders the two so that cannot happen today; asking
    // `events` to build its own surface makes it independent of that order.
    let emitters = crate::events::namespace(context);
    let constructor = rts_core_rwk::entry::get_member(context, emitters, "EventEmitter");
    let prototype = rts_core_rwk::entry::get_member(context, constructor, "prototype");
    rts_core_rwk::entry::set_prototype_in(context, namespace, prototype);
    // `EventEmitter`'s storage, which its constructor would otherwise have
    // installed: without it `on` writes a listener array onto `undefined` and
    // every registration is silently dropped.
    let events = rts_core_rwk::entry::make_object(context);
    rts_core_rwk::entry::put_member(context, namespace, "__events__", events);

    rts_core_rwk::entry::declare_loop_source(context, "node:process.nextTick", lifecycle::source);
    namespace
}

/// The `process` object this thread built, or `0` before one exists.
fn object() -> u64 {
    PROCESS.with(Cell::get)
}

/// Fires an event on `process`, answering whether any listener took it.
///
/// Reaches `EventEmitter.prototype.emit` through an ordinary property read and
/// calls it, rather than reading `__events__` here: two walks of one listener
/// registry is the duplication `reuse-check` exists to stop, and the `once`
/// bookkeeping lives in that one implementation.
///
/// The borrow is released before the call. `emit` runs user code, and calling
/// it under [`rts_core_rwk::entry::with_runtime`] is the nested borrow that
/// aborts the process from an `extern "C"` frame.
fn emit(event: &str, argument: u64) -> bool {
    let process = object();
    if process == 0 {
        return false;
    }
    let (emitter, name) = rts_core_rwk::entry::with_runtime(|context| {
        (
            rts_core_rwk::entry::get_member(context, process, "emit"),
            rts_core_rwk::entry::make_string(context, event),
        )
    });
    let absent = rts_core_rwk::entry::undefined_value();
    if emitter == absent {
        return false;
    }
    let answered = rts_core_rwk::entry::call(emitter, process, name, argument, absent, absent);
    rts_core_rwk::entry::to_boolean(answered)
}
