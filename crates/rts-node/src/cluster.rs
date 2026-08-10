//! `node:cluster` — process spawning wearing `cluster`'s name.
//!
//! # Reuse-check
//!
//! `rts-cranelift` has nothing shaped like a process table (checked
//! `src/sched/`, `src/frame/` — both are about scheduling a JS continuation,
//! not an OS process). `rts_core::entry::Context` holds no process/worker
//! table either. The nearest existing shape is
//! [`crate::child_process::spawn_async`]'s `PROCESSES` registry plus its
//! queue-then-pump event delivery — this module is that same shape, restated
//! for `Worker` instead of `ChildProcess`, because `cluster.fork()` really is
//! `child_process`'s spawn with a second name (`docs/reference/node/
//! cluster.md` §4: "Workers are spawned via `child_process.fork()`"). It is
//! not built by calling into `child_process` — this crate owns that folder as
//! a sibling module this pass may not edit, and `std::process::Command` is
//! the whole mechanism either module needs, so restating it here is not a
//! second value encoding, only a second call site of the same one.
//!
//! # What this module is honest about
//!
//! **There is no IPC.** Real `node:cluster` exists to let workers share a
//! listening socket, and that needs a byte channel between primary and
//! worker plus, for the round-robin/handle-off scheduling policies, real
//! cross-process socket-handle transfer (`SCM_RIGHTS`/`WSADuplicateSocket`).
//! None of that exists here. What is built is the primary/worker distinction
//! and real OS process spawning — `fork()` genuinely starts another OS
//! process running this same program — plus the lifecycle events a spawn
//! produces on its own (`'fork'`, `'exit'`) and one best-effort approximation
//! (`'online'`, fired right after `'fork'` rather than on a real readiness
//! handshake, because no handshake channel exists to wait on). `worker.send`,
//! `'message'`, `'listening'`, `.disconnect()`/`.isConnected()`, and both
//! scheduling policies (`SCHED_RR`/`SCHED_NONE` are stored as inert numbers,
//! never consulted) are refused by name below rather than half-built into
//! something that looks like it shares a socket and does not.
//!
//! # WHEN a listener runs
//!
//! Same rule as [`crate::child_process::spawn_async`], for the same reason
//! (no event loop, and calling into JS from the waiter thread would be a
//! cross-thread call into a borrow that is not there — an abort): every
//! event is queued by a background thread and delivered only from [`pump`],
//! which every native in this module calls first. A program that never calls
//! a second `cluster` function after `fork()` never observes `'fork'`'s own
//! listener, if one was attached after the call returned but before the next
//! pump.
//!
//! # Not implemented, by name
//!
//! - **`worker.send`/`.disconnect`/`.isConnected`/`.isDead`,
//!   `cluster.disconnect(callback)`, the `'message'`/`'listening'`/
//!   `'disconnect'`/`'setup'` events, `sendHandle`/socket handoff.** All need
//!   the IPC channel described above; none exists.
//! - **`schedulingPolicy` enforcement.** `SCHED_RR`/`SCHED_NONE` are stored
//!   and read back, never acted on — there is no shared listening socket to
//!   schedule connections onto.
//! - **`env` merging on `fork()`.** The argument is accepted and ignored; the
//!   worker inherits the primary's environment plus the unique-id marker
//!   only. Merging needs walking an arbitrary JS object's own keys through
//!   [`rts_core::entry::own_keys`] and reading each back by name, which
//!   is buildable but not attempted in this pass — named rather than silently
//!   dropped.
//! - **`exitedAfterDisconnect`.** Tri-state per the spec, but with no
//!   `.disconnect()` to set it `true`, this module's workers only ever reach
//!   `false` (killed/exited without a graceful disconnect) or stay
//!   `undefined` (still alive) — never `true`.
//! - **`inspectPort`, `uid`/`gid`, `windowsHide`, `stdio`/`silent`.** Accepted
//!   on `setupPrimary`'s settings object as inert data, never applied to the
//!   spawned `Command`.

use rts_core::entry::{self, Provided};
use std::collections::{HashMap, VecDeque};
use std::process::Child;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Set in a worker's own environment by [`fork`]; its absence is what
/// [`is_primary`] reads. `NODE_UNIQUE_ID` is real Node's own name for this —
/// reused rather than invented, since nothing here needs a distinct one.
const UNIQUE_ID_VAR: &str = "NODE_UNIQUE_ID";

enum Queued {
    Fork,
    Online,
    Exit { code: Option<i32>, signal: Option<&'static str> },
}

struct WorkerEntry {
    child: Option<Child>,
    instance: u64,
    queue: VecDeque<Queued>,
    reaped: bool,
}

static WORKERS: Mutex<Option<HashMap<u64, WorkerEntry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, WorkerEntry>) -> T) -> T {
    let mut guard = WORKERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

fn is_primary() -> bool {
    std::env::var(UNIQUE_ID_VAR).is_err()
}

const WORKER_METHODS: &[(&str, Provided)] = &[("kill", kill), ("destroy", kill), ("ref", noop_self), ("unref", noop_self)];

/// The namespace `node:cluster` is.
pub fn namespace(context: &mut entry::Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("fork", fork),
        ("setupPrimary", setup_primary),
        ("setupMaster", setup_primary),
        ("disconnect", disconnect_all),
    ];
    let namespace = entry::make_namespace(context, members);
    // The `cluster` module object is itself an `EventEmitter` in real Node
    // (`'fork'`/`'online'`/`'exit'` fire ON it, not on a separate emitter) —
    // linked here the same way `child_process`'s `ChildProcess` links onto
    // it in `spawn_async::spawn`.
    let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
    entry::set_prototype_in(context, namespace, event_emitter);
    // "cluster.Worker" rather than "Worker": `node:worker_threads` registers a
    // DIFFERENT class under that bare name (a real thread's handle, with its
    // own method table), and `make_prototype` is idempotent by name — a bare
    // "Worker" here would hand cluster's methods to worker_threads instances or
    // the reverse, depending only on install order. See `make_prototype`'s doc
    // for the collision this would otherwise be.
    entry::make_prototype(context, "cluster.Worker", WORKER_METHODS);

    let primary = entry::boolean_value(is_primary());
    entry::put_member(context, namespace, "isPrimary", primary);
    entry::put_member(context, namespace, "isMaster", primary);
    let worker = entry::boolean_value(!is_primary());
    entry::put_member(context, namespace, "isWorker", worker);

    let sched_rr = entry::make_number(2.0);
    let sched_none = entry::make_number(1.0);
    entry::put_member(context, namespace, "SCHED_RR", sched_rr);
    entry::put_member(context, namespace, "SCHED_NONE", sched_none);
    // Windows has no libuv IOCP round-robin story either, real or ours —
    // matching Node's own default split, per `docs/reference/node/cluster.md` §4.
    let default_policy = if cfg!(windows) { sched_none } else { sched_rr };
    entry::put_member(context, namespace, "schedulingPolicy", default_policy);

    let settings = entry::make_object(context);
    entry::put_member(context, namespace, "settings", settings);
    let workers = entry::make_object(context);
    entry::put_member(context, namespace, "workers", workers);
    let self_worker = entry::undefined_in(context);
    entry::put_member(context, namespace, "worker", self_worker);
    namespace
}

/// Delivers every queued event, oldest first per worker. Drops [`WORKERS`]'s
/// lock before calling anything — the same discipline
/// [`crate::child_process::spawn_async::pump`] documents, for the same
/// reason: a listener that itself calls another `cluster` native must not
/// deadlock on a lock this function still holds.
fn pump(namespace: u64) {
    let due: Vec<(u64, Vec<Queued>)> =
        with_table(|table| table.iter_mut().filter(|(_, entry)| !entry.queue.is_empty()).map(|(&id, entry)| (id, entry.queue.drain(..).collect())).collect());
    if due.is_empty() {
        return;
    }
    // One borrow to read `emit` off the namespace object, closed before any
    // listener runs — [`entry::call`] is ambient and opens its own borrow
    // per call, so it must never run while this one is still held (the same
    // two-phase shape [`crate::child_process::spawn_async::pump`] and
    // [`crate::events::emit`] both document for the identical reason).
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, namespace, "emit"));
    let absent = entry::undefined_value();
    for (id, events) in due {
        let instance = with_table(|table| table.get(&id).map(|entry| entry.instance));
        let Some(instance) = instance else { continue };
        for queued in events {
            match queued {
                Queued::Fork => {
                    let name = entry::with_runtime(|context| entry::make_string(context, "fork"));
                    entry::call(emit_fn, namespace, name, instance, absent, absent);
                }
                Queued::Online => {
                    let name = entry::with_runtime(|context| entry::make_string(context, "online"));
                    entry::call(emit_fn, namespace, name, instance, absent, absent);
                }
                Queued::Exit { code, signal } => {
                    let code_value = code.map(|code| entry::make_number(f64::from(code))).unwrap_or_else(entry::null_value);
                    entry::with_runtime(|context| remove_worker(context, namespace, id));
                    let name = entry::with_runtime(|context| entry::make_string(context, "exit"));
                    let signal_value = signal.map(|signal| entry::with_runtime(|context| entry::make_string(context, signal))).unwrap_or_else(entry::null_value);
                    entry::call(emit_fn, namespace, name, instance, code_value, signal_value);
                }
            }
        }
    }
}

fn rebuild_workers(context: &mut entry::Context, namespace: u64) {
    let workers = entry::make_object(context);
    with_table(|table| {
        for (&id, entry_) in table.iter() {
            if entry_.reaped {
                continue;
            }
            let key = id.to_string();
            entry::put_member(context, workers, &key, entry_.instance);
        }
    });
    entry::put_member(context, namespace, "workers", workers);
}

fn remove_worker(context: &mut entry::Context, namespace: u64, id: u64) {
    with_table(|table| {
        if let Some(entry_) = table.get_mut(&id) {
            entry_.reaped = true;
        }
    });
    rebuild_workers(context, namespace);
}

/// `cluster.fork(env?)` — genuinely spawns an OS process re-running this same
/// program, with [`UNIQUE_ID_VAR`] set so the child's own `isPrimary` reads
/// `false`. See the module doc: `env` is accepted, not merged.
extern "C" fn fork(_e: u64, namespace: u64, _env: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    pump(namespace);
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let program = std::env::current_exe().ok();
    let args: Vec<String> = std::env::args().skip(1).collect();

    let (child, queued) = match program {
        Some(program) => {
            let mut command = std::process::Command::new(program);
            command.args(&args).env(UNIQUE_ID_VAR, id.to_string());
            match command.spawn() {
                Ok(child) => (Some(child), Queued::Fork),
                Err(_) => (None, Queued::Fork),
            }
        }
        None => (None, Queued::Fork),
    };
    let has_child = child.is_some();
    let pid = child.as_ref().map(std::process::Child::id);

    let instance = entry::with_runtime(|context| {
        let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
        let prototype = entry::make_prototype(context, "cluster.Worker", WORKER_METHODS);
        entry::set_prototype_in(context, prototype, event_emitter);
        let instance = entry::make_instance(context, prototype);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "id", id_value);
        entry::put_member(context, instance, "__workerId", id_value);
        let process = entry::make_object(context);
        let pid_value = pid.map(|pid| entry::make_number(f64::from(pid))).unwrap_or_else(|| entry::undefined_in(context));
        entry::put_member(context, process, "pid", pid_value);
        entry::put_member(context, instance, "process", process);
        let exited_after = entry::undefined_in(context);
        entry::put_member(context, instance, "exitedAfterDisconnect", exited_after);
        instance
    });

    let mut queue = VecDeque::new();
    queue.push_back(queued);
    if has_child {
        queue.push_back(Queued::Online);
    }
    with_table(|table| {
        table.insert(id, WorkerEntry { child, instance, queue, reaped: false });
    });
    if has_child {
        spawn_waiter(id);
    }
    entry::with_runtime(|context| rebuild_workers(context, namespace));
    instance
}

fn spawn_waiter(id: u64) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(20));
            let done = with_table(|table| {
                let Some(entry) = table.get_mut(&id) else { return true };
                if entry.reaped {
                    return true;
                }
                let Some(child) = entry.child.as_mut() else { return true };
                match child.try_wait() {
                    Ok(Some(status)) => {
                        entry.queue.push_back(Queued::Exit { code: status.code(), signal: signal_of(&status) });
                        entry.child = None;
                        true
                    }
                    Ok(None) => false,
                    Err(_) => true,
                }
            });
            if done {
                break;
            }
        }
    });
}

#[cfg(unix)]
fn signal_of(status: &std::process::ExitStatus) -> Option<&'static str> {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map(|_| "SIGTERM")
}

#[cfg(not(unix))]
fn signal_of(_status: &std::process::ExitStatus) -> Option<&'static str> {
    None
}

/// `cluster.setupPrimary(settings?)` / `cluster.setupMaster(settings?)` —
/// stores whatever object it is given as `cluster.settings`, without merging
/// or applying any of it to a future `fork()` (see the module doc).
extern "C" fn setup_primary(_e: u64, namespace: u64, settings: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let target = if entry::is_object(context, settings) { settings } else { entry::make_object(context) };
        entry::put_member(context, namespace, "settings", target);
    });
    entry::undefined_value()
}

/// `cluster.disconnect(callback?)` — with no IPC channel, there is nothing to
/// disconnect; every live worker is simply killed and `callback` is invoked
/// once all are reaped. A real graceful drain is refused (module doc).
extern "C" fn disconnect_all(_e: u64, namespace: u64, callback: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let ids: Vec<u64> = with_table(|table| table.iter().filter(|(_, entry)| !entry.reaped).map(|(&id, _)| id).collect());
    for id in ids {
        with_table(|table| {
            if let Some(entry) = table.get_mut(&id) {
                if let Some(child) = entry.child.as_mut() {
                    let _ = child.kill();
                }
            }
        });
    }
    let undefined = entry::undefined_value();
    entry::call(callback, namespace, undefined, undefined, undefined, undefined);
    undefined
}

fn worker_id_of(this: u64) -> Option<u64> {
    let value = entry::with_runtime(|context| entry::get_member(context, this, "__workerId"));
    entry::number_of(value).map(|value| value as u64)
}

/// `worker.kill(signal?)` / `worker.destroy(signal?)` — like
/// `child_process`'s own `.kill()`, always a forced termination regardless of
/// `signal` (see [`crate::child_process`]'s module doc for the same limit).
extern "C" fn kill(_e: u64, this: u64, _signal: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(id) = worker_id_of(this) else {
        return entry::undefined_value();
    };
    with_table(|table| {
        if let Some(entry) = table.get_mut(&id) {
            if let Some(child) = entry.child.as_mut() {
                let _ = child.kill();
            }
        }
    });
    entry::undefined_value()
}

extern "C" fn noop_self(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}
