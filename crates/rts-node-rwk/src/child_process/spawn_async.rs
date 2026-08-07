//! `spawn()` and `ChildProcess` — the event-driven half.
//!
//! # WHEN a listener runs — read this before relying on `spawn`
//!
//! This engine has no event loop, and a child's own waiter thread is not the
//! JS thread — `rts_core_rwk::entry::current`'s own doc says calling into JS
//! from any other thread aborts the process. So, exactly like
//! [`crate::fs::watch`] (read its module doc; this is the same answer to the
//! same problem): the waiter thread pushes a plain, non-JS record into
//! [`PROCESSES`], and [`pump`] — called at the start of `text()`/`string()`
//! in [`super::shared`], which every native in this module calls first — is
//! the only place a listener actually runs. **A queued event's listener
//! fires just before the NEXT call any `node:child_process` native makes**,
//! not the instant the OS event happens. `'spawn'` and `'error'` are
//! therefore not delivered synchronously from `spawn()` either — even
//! though this thread already knows the answer when `spawn()` returns —
//! because delivering them immediately would run a listener attached AFTER
//! `spawn()` returns before that listener exists, which is not what Node
//! does. A program that calls no further `child_process` function after
//! `spawn()` never observes any of its listeners fire; that is a real,
//! named limit, not a silently-broken feature.
//!
//! # What is NOT implemented here, and why
//!
//! - **Piped stdio, `'data'` events, `.stdin`/`.stdout`/`.stderr`.** Always
//!   `null`; every child inherits the parent's stdio unless `stdio: 'ignore'`
//!   asks otherwise. A real `Readable`/`Writable` facade needs the same
//!   listener-delivery problem this module already has, times three
//!   concurrent streams, and `docs/reference/node/child_process.md` §5.3
//!   defers exactly this to a dedicated pass — not attempted here.
//! - **`exec`, `execFile`, `fork`.** `exec`/`execFile` need the piped-stdio
//!   accumulation above (to hand a callback `stdout`/`stderr`); `fork` needs
//!   a self-re-exec protocol and an IPC channel neither of which exists yet.
//!   Refused by name rather than half-built.
//! - **`send`/`disconnect`/`channel`/`connected`.** No IPC channel exists —
//!   `fork()` is what would create one.
//! - **`[Symbol.dispose]`.** `rts_core_rwk::entry::modules` (the API this
//!   crate is restricted to) has no way to construct a `Symbol`-keyed
//!   member from outside the runtime crate.
//! - **`uid`/`gid`/`detached`/`AbortSignal`/`timeout`.** `spawnSync`'s
//!   `capture` module has a poll loop to hang a `timeout` off; `spawn()`
//!   here has no equivalent loop (the whole point is not to block), and
//!   wiring one to a background timer thread + `KILL` is future work, named
//!   rather than silently ignored.
//! - **Real signal delivery in `kill()`.** Same limit as
//!   [`super::capture`]'s module doc: `Child::kill()` is the only
//!   termination `std` gives without `libc`, so `kill()` always forcibly
//!   terminates the child regardless of the signal name passed in.

use std::collections::{HashMap, VecDeque};
use std::process::Child;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rts_core_rwk::entry::{self, Provided};

use super::command;
use super::shared::string;

/// One queued, JS-free observation — the same discipline
/// [`crate::fs::watch`]'s `Queued` documents: never a JS value, so the
/// waiter thread never needs a context.
enum Queued {
    Spawn,
    Error { message: String, code: &'static str },
    Exit { code: Option<i32>, signal: Option<&'static str> },
}

struct ProcEntry {
    /// The thread that made it.
    ///
    /// This table is process-wide and every thread running a program pumps it,
    /// so without this one thread delivers another thread's event: it emits
    /// onto a JS instance naming cells in a region it does not have. Found in
    /// `node:worker_threads` first, where two parallel tests were doing it to
    /// each other. Every table of this shape needs it.
    owner: std::thread::ThreadId,
    child: Option<Child>,
    instance: u64,
    queue: VecDeque<Queued>,
    /// Set once the `Exit` event has been queued — a second poll must not
    /// queue a second one for a child already reaped.
    reaped: bool,
}

static PROCESSES: Mutex<Option<HashMap<u64, ProcEntry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, ProcEntry>) -> T) -> T {
    let mut guard = PROCESSES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// Delivers every queued event, oldest first per process, dropping
/// [`PROCESSES`]'s lock before calling anything — a listener that itself
/// calls another `child_process` native must not deadlock on a lock this
/// function still holds. Called from [`super::shared::text`]; see the
/// module doc for exactly when that puts a listener on the JS thread.
pub(super) fn pump() {
    let due: Vec<(u64, Vec<Queued>)> =
        with_table(|table| table.iter_mut().filter(|(_, entry)| entry.owner == std::thread::current().id()).filter(|(_, entry)| !entry.queue.is_empty()).map(|(&id, entry)| (id, entry.queue.drain(..).collect())).collect());
    if due.is_empty() {
        return;
    }
    let absent = entry::undefined_value();
    for (id, events) in due {
        let instance = with_table(|table| table.get(&id).map(|entry| entry.instance));
        let Some(instance) = instance else { continue };
        let emit_fn = entry::with_runtime(|context| entry::get_member(context, instance, "emit"));
        for queued in events {
            match queued {
                Queued::Spawn => {
                    let name = string("spawn");
                    entry::call(emit_fn, instance, name, absent, absent, absent);
                }
                Queued::Error { message, code } => {
                    let error = entry::with_runtime(|context| {
                        let object = entry::make_object(context);
                        let message_value = entry::make_string(context, &message);
                        entry::put_member(context, object, "message", message_value);
                        let code_value = entry::make_string(context, code);
                        entry::put_member(context, object, "code", code_value);
                        object
                    });
                    let name = string("error");
                    entry::call(emit_fn, instance, name, error, absent, absent);
                }
                Queued::Exit { code, signal } => {
                    let code_value = code.map(|code| entry::make_number(f64::from(code))).unwrap_or_else(entry::null_value);
                    let signal_value = signal.map(string).unwrap_or_else(entry::null_value);
                    entry::with_runtime(|context| {
                        entry::put_member(context, instance, "exitCode", code_value);
                        entry::put_member(context, instance, "signalCode", signal_value);
                    });
                    let exit_name = string("exit");
                    entry::call(emit_fn, instance, exit_name, code_value, signal_value, absent);
                    // No stdio streams exist to wait on draining (see the
                    // module doc), so `'close'` fires immediately after
                    // `'exit'` rather than at a later, separately-observed
                    // point — a named narrowing of Node's own two-step
                    // guarantee.
                    let close_name = string("close");
                    entry::call(emit_fn, instance, close_name, code_value, signal_value, absent);
                }
            }
        }
    }
}

pub(super) const METHODS: &[(&str, Provided)] = &[("kill", kill), ("ref", noop_self), ("unref", noop_self)];

/// `spawn(command, args?, options?)`.
pub(super) extern "C" fn spawn(_e: u64, _this: u64, command_value: u64, args_value: u64, options: u64, _a3: u64) -> u64 {
    let Some(program) = super::shared::text(command_value) else {
        return entry::undefined_value();
    };
    let args = super::shared::string_array(args_value);
    let spec = command::spec_of(&program, args, options);
    let mut cmd = command::build(&spec, options);
    if command::stdio_mode(options) == command::StdioMode::Ignore {
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    }
    // The default and `'pipe'` cases both inherit — see the module doc's
    // "not implemented" section for why there is no third branch here.

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let (queued, child, pid) = match cmd.spawn() {
        Ok(child) => {
            let pid = child.id();
            (Queued::Spawn, Some(child), Some(pid))
        }
        Err(error) => {
            let code = match error.kind() {
                std::io::ErrorKind::NotFound => "ENOENT",
                std::io::ErrorKind::PermissionDenied => "EACCES",
                _ => "UNKNOWN",
            };
            (Queued::Error { message: error.to_string(), code }, None, None)
        }
    };

    let instance = entry::with_runtime(|context| {
        let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
        let prototype = entry::make_prototype(context, "ChildProcess", METHODS);
        entry::set_prototype_in(context, prototype, event_emitter);
        let instance = entry::make_instance(context, prototype);
        let events = entry::make_object(context);
        entry::put_member(context, instance, "__events__", events);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__procId", id_value);
        let pid_value = pid.map(|pid| entry::make_number(f64::from(pid))).unwrap_or_else(|| entry::undefined_in(context));
        entry::put_member(context, instance, "pid", pid_value);
        let killed = entry::boolean_value(false);
        entry::put_member(context, instance, "killed", killed);
        let exit_code = entry::null_in(context);
        entry::put_member(context, instance, "exitCode", exit_code);
        let signal_code = entry::null_in(context);
        entry::put_member(context, instance, "signalCode", signal_code);
        let spawnfile = entry::make_string(context, &spec.program);
        entry::put_member(context, instance, "spawnfile", spawnfile);
        let arg_values: Vec<u64> = spec.args.iter().map(|arg| entry::make_string(context, arg)).collect();
        let spawnargs = entry::make_array_in(context, arg_values);
        entry::put_member(context, instance, "spawnargs", spawnargs);
        let connected = entry::boolean_value(false);
        entry::put_member(context, instance, "connected", connected);
        instance
    });

    let has_child = child.is_some();
    with_table(|table| {
        table.insert(id, ProcEntry { owner: std::thread::current().id(), child, instance, queue: VecDeque::from([queued]), reaped: false });
    });
    if has_child {
        spawn_waiter(id);
    }
    instance
}

/// Polls the child for exit, off the JS thread — see the module doc for why
/// it never calls into JS itself.
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
                        entry.queue.push_back(Queued::Exit { code: status.code(), signal: super::capture::signal_of(status) });
                        entry.reaped = true;
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

fn proc_id_of(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, string("__procId"));
    entry::number_of(value).map(|value| value as u64)
}

/// `subprocess.kill(signal?)` — see the module doc: always a forced
/// termination, regardless of `signal`.
extern "C" fn kill(_e: u64, this: u64, _signal: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(id) = proc_id_of(this) else {
        return entry::boolean_value(false);
    };
    let delivered = with_table(|table| match table.get_mut(&id) {
        Some(entry) => match entry.child.as_mut() {
            Some(child) => child.kill().is_ok(),
            None => false,
        },
        None => false,
    });
    if delivered {
        entry::with_runtime(|context| {
            let killed = entry::boolean_value(true);
            entry::put_member(context, this, "killed", killed);
        });
    }
    entry::boolean_value(delivered)
}

extern "C" fn noop_self(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}

/// This module as a loop source: deliver what its background threads queued,
/// then say whether any is still live.
///
/// `Blocked` and never `In`, because what this waits on is the outside world
/// and that has no deadline to report. A `Blocked` source is pumped on every
/// pass and does NOT hold the program open — `entry::loops` says why. It means
/// a program whose last act is to start one of these still ends, where Node
/// would keep running; the alternative is every fixture hanging on a listener
/// nothing closes.
pub fn source() -> entry::Pending {
    pump();
    let mine = std::thread::current().id();
    let live = with_table(|table| {
        table.values().any(|entry| entry.owner == mine && !entry.reaped)
    });
    match live {
        true => entry::Pending::Blocked,
        false => entry::Pending::Idle,
    }
}
