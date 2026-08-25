//! `spawn()` and `ChildProcess` — the event-driven half.
//!
//! # WHEN a listener runs — read this before relying on `spawn`
//!
//! This engine has no event loop, and a child's own waiter thread is not the
//! JS thread — `rts_core::entry::current`'s own doc says calling into JS
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
//! - ~~**Piped stdio**~~ — it exists, in [`super::stdio`], and the entry stays
//!   because its reason is the instructive part: *"a real `Readable`/`Writable`
//!   facade needs the same listener-delivery problem this module already has,
//!   times three concurrent streams"*. The delivery problem was solved for
//!   everybody by `entry::loops` — a source registers a `fn` and the host pumps
//!   it on the program's thread — so what was left was not a mechanism but
//!   three objects. The default is `'pipe'` now, as Node's is; it used to
//!   inherit for both the default and the explicit `'pipe'`, which is why
//!   `child.stdout` was `null` and a program's first line after `spawn()` read
//!   a property of `undefined`.
//! - **`fork`.** Needs a self-re-exec protocol and an IPC channel, neither of
//!   which exists yet. Refused by name rather than half-built.
//! - **`send`/`disconnect`/`channel`/`connected`.** No IPC channel exists —
//!   `fork()` is what would create one.
//! - **`[Symbol.dispose]`.** `rts_core::entry::modules` (the API this
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

use rts_core::entry::{self, Provided};

use super::command;
use super::shared::string;

/// One queued, JS-free observation — the same discipline
/// [`crate::fs::watch`]'s `Queued` documents: never a JS value, so the
/// waiter thread never needs a context.
enum Queued {
    Spawn,
    Error { message: String, code: &'static str },
    Exit { code: Option<i32>, signal: Option<&'static str> },
    /// Bytes a reader thread took off one of the child's pipes.
    ///
    /// A `Vec<u8>` and never a JS value, which is the rule this table is built
    /// on: the thread that produced it has no context to make one in.
    Data { fd: super::stdio::Fd, bytes: Vec<u8> },
    /// One pipe reached its end.
    StreamEnd { fd: super::stdio::Fd },
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
    /// The write side of the child's stdin, while it is open.
    ///
    /// Held here and not on the JS object because it is an OS handle: a value
    /// crosses regions, a handle does not.
    stdin: Option<std::process::ChildStdin>,
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

/// Queues a chunk a reader thread took off a pipe. `false` once the process is
/// gone, which is the reader's signal to stop reading for nobody.
pub(super) fn queue_data(id: u64, fd: super::stdio::Fd, bytes: Vec<u8>) -> bool {
    with_table(|table| match table.get_mut(&id) {
        Some(entry) => {
            entry.queue.push_back(Queued::Data { fd, bytes });
            true
        }
        None => false,
    })
}

/// Queues the end of one pipe.
pub(super) fn queue_end(id: u64, fd: super::stdio::Fd) {
    with_table(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.queue.push_back(Queued::StreamEnd { fd });
        }
    });
}

/// Writes to the child's stdin, if it is still open.
pub(super) fn write_stdin(id: u64, bytes: &[u8]) -> bool {
    with_table(|table| match table.get_mut(&id).and_then(|entry| entry.stdin.as_mut()) {
        Some(handle) => super::stdio::write_to(handle, bytes),
        None => false,
    })
}

/// Drops the write side, which is what tells a child reading to end-of-input
/// that there is no more.
pub(super) fn close_stdin(id: u64) {
    with_table(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.stdin = None;
        }
    });
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
                // A chunk goes to the STREAM object, not to the child: a
                // program listens on `child.stdout`, and emitting on the child
                // would put `'data'` where nothing listens for it.
                Queued::Data { fd, bytes } => {
                    let stream = entry::with_runtime(|context| {
                        entry::get_member(context, instance, fd.member())
                    });
                    if stream == absent || destroyed(stream) {
                        continue;
                    }
                    let chunk = chunk_value(stream, &bytes);
                    let emit = entry::with_runtime(|context| {
                        entry::get_member(context, stream, "emit")
                    });
                    let name = string("data");
                    entry::call(emit, stream, name, chunk, absent, absent);
                    // `pipe` is a write into the destination, done here rather
                    // than by a listener: the destination is an ordinary
                    // Writable and this is the only place holding the chunk.
                    let destination =
                        entry::with_runtime(|context| entry::get_member(context, stream, "__pipe"));
                    if destination != absent {
                        let write = entry::with_runtime(|context| {
                            entry::get_member(context, destination, "write")
                        });
                        if entry::with_runtime(|context| entry::is_callable_in(context, write)) {
                            entry::call(write, destination, chunk, absent, absent, absent);
                        }
                    }
                }
                Queued::StreamEnd { fd } => {
                    let stream = entry::with_runtime(|context| {
                        entry::get_member(context, instance, fd.member())
                    });
                    if stream == absent || destroyed(stream) {
                        continue;
                    }
                    let emit = entry::with_runtime(|context| {
                        entry::get_member(context, stream, "emit")
                    });
                    let name = string("end");
                    entry::call(emit, stream, name, absent, absent, absent);
                    let name = string("close");
                    entry::call(emit, stream, name, absent, absent, absent);
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
    // Node's default for `spawn` is `'pipe'`, and this used to inherit for both
    // the default and the explicit `'pipe'` — which is why `child.stdout` was
    // `null` and every program's first line after `spawn()` read a property of
    // `undefined`. The three branches are now the three modes.
    let mode = command::stdio_mode(options);
    match mode {
        command::StdioMode::Ignore => {
            cmd.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }
        command::StdioMode::Pipe => {
            cmd.stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
        }
        // `'inherit'` is the child writing where this process writes, which is
        // what `Command` does by default — nothing to set.
        command::StdioMode::Inherit => {}
    }

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let mut piped = None;
    let (queued, child, pid) = match cmd.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            // TAKEN, not borrowed: the reader threads own these for as long as
            // the pipes are open, and the table keeps only the child handle it
            // needs for `try_wait` and `kill`.
            piped = Some((child.stdout.take(), child.stderr.take(), child.stdin.take()));
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
        // The three stdio members, and `null` where Node puts `null`: a child
        // spawned with `'inherit'` or `'ignore'` genuinely has no stream on
        // this side, and an object there would be one a program could listen
        // on and never hear from.
        let (out, err, input) = match mode {
            command::StdioMode::Pipe => (
                super::stdio::readable(context, id, super::stdio::Fd::Out),
                super::stdio::readable(context, id, super::stdio::Fd::Err),
                super::stdio::writable(context, id),
            ),
            _ => {
                let absent = entry::null_in(context);
                (absent, absent, absent)
            }
        };
        entry::put_member(context, instance, "stdout", out);
        entry::put_member(context, instance, "stderr", err);
        entry::put_member(context, instance, "stdin", input);
        // `child.stdio` is the array form of the same three, which is what a
        // program indexes when it does not know their names.
        let trio = entry::make_array_in(context, vec![input, out, err]);
        entry::put_member(context, instance, "stdio", trio);
        instance
    });

    let has_child = child.is_some();
    let (out, err, input) = match piped {
        Some((out, err, input)) => (out, err, input),
        None => (None, None, None),
    };
    with_table(|table| {
        // A spawn that FAILED is already finished: there is no process to wait
        // for, and `reaped: false` would make `source` hold the program open
        // for a child that never existed — forever, since nothing will ever
        // reap it. Five files of Node's own suite timed out on exactly that,
        // all of them spawning something this machine does not have.
        let reaped = child.is_none();
        table.insert(id, ProcEntry { owner: std::thread::current().id(), child, stdin: input, instance, queue: VecDeque::from([queued]), reaped });
    });
    if has_child {
        // The readers start AFTER the entry is in the table: a thread that
        // queues before the entry exists would find nothing to queue onto and
        // stop, which is exactly the chunk a program was waiting for.
        super::stdio::attach(id, out, err);
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

/// Whether a stream object has been destroyed, and so should get no more.
fn destroyed(stream: u64) -> bool {
    entry::with_runtime(|context| entry::get_member(context, stream, "destroyed"))
        == entry::boolean_value(true)
}

/// One chunk as the stream's own encoding says it should arrive.
///
/// A `Buffer` unless `setEncoding` named one, which is Node's rule and the
/// reason the encoding lives on the stream object: two streams over one child
/// decode independently.
fn chunk_value(stream: u64, bytes: &[u8]) -> u64 {
    let encoding =
        entry::with_runtime(|context| entry::get_member(context, stream, "__encoding"));
    match super::shared::text(encoding) {
        // The name is not consulted beyond "there is one": this engine's
        // strings are UTF-8 and a `latin1` request would need a second decoder
        // that nothing here has. Stated rather than silently applied — a
        // program asking for `latin1` gets UTF-8 and can see it in the bytes.
        Some(_) => entry::with_runtime(|context| {
            let text = String::from_utf8_lossy(bytes);
            entry::make_string(context, &text)
        }),
        None => entry::with_runtime(|context| entry::make_buffer(context, bytes)),
    }
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
/// # Why this HOLDS the program open, where a socket does not
///
/// It answered `Blocked` — pumped every pass, holding nothing open — with the
/// reason that *"the alternative is every fixture hanging on a listener nothing
/// closes"*. That reason is a listening SERVER's, and it does not transfer: a
/// server waits on something that may never come, and a child process ends. So
/// the wait here is bounded by the child's own lifetime, which is exactly what
/// `Pending::In` is for.
///
/// Keeping `Blocked` had a cost that only became visible once the streams were
/// real: a program whose last statement was `spawn()` ended before the child
/// had written a byte, so `'data'` and `'exit'` fired for nobody. That is what
/// Node keeps a process alive FOR, and it is the whole point of spawning
/// something and listening to it.
///
/// The queue is part of "live" and not an afterthought: a child that has exited
/// may still have chunks nobody has been handed yet, and going `Idle` there
/// would end the program between the last `'data'` and the `'exit'` that
/// follows it.
///
/// 10 ms because it is a poll of a table, not a wait on an event: the waiter
/// thread already sleeps 20 ms between `try_wait`s, so asking twice as often
/// costs one lock and means a child's exit is never more than a tick late.
pub fn source() -> entry::Pending {
    pump();
    let mine = std::thread::current().id();
    let live = with_table(|table| {
        table
            .values()
            .any(|entry| entry.owner == mine && (!entry.reaped || !entry.queue.is_empty()))
    });
    match live {
        true => entry::Pending::In(Duration::from_millis(10)),
        false => entry::Pending::Idle,
    }
}
