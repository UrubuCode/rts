//! `node:worker_threads` — a `Worker` is a real OS thread running a real
//! engine, and what crosses between them is a copy.
//!
//! # Why this could not be written until now
//!
//! It needed two things this crate could not reach. A worker runs JavaScript,
//! so it needs a compiler — and only the host has one, handed down as
//! `entry::evaluator()`. And a second context had to be able to exist at all:
//! the thread-local holding one was a single slot that installing OVER
//! destroyed, which is the same defect that kept `node:vm` unregistered. It is
//! a stack now.
//!
//! # What a worker is here
//!
//! `new Worker(source, { eval: true })` starts an OS thread, and that thread
//! calls the host's evaluator: it compiles the source, installs its own
//! context, its own region and its own copy of every module, runs to the end,
//! and goes away. Nothing is shared — not a heap, not a cell, not a lock — so
//! nothing about a worker can corrupt the thread that made it.
//!
//! **`eval: false` is refused by name.** A path would have to be read and
//! resolved the way an import is, and this crate has no loader; `node:module`'s
//! `createRequire` is refused for the same missing piece. A `Worker` over a
//! filename emits `'error'` saying so rather than starting a thread that finds
//! nothing.
//!
//! # What crosses, and the shape of the crossing
//!
//! A copy, never a reference: a reference belongs to the region that made it,
//! and a worker's region is on another thread. [`portable`] states exactly what
//! it carries — `undefined`, `null`, booleans, numbers, strings, arrays and
//! plain objects — and turns anything else into a named marker rather than a
//! value that arrives shaped wrong.
//!
//! Delivery is the rule every threaded module here follows: the worker's thread
//! never calls a listener. It queues native data, and the parent turns that
//! into `'message'`/`'error'`/`'exit'` on its own thread.
//!
//! # The direction that is synchronous, and why
//!
//! Parent → worker is a **poll**, not an event. `worker.postMessage(v)` queues,
//! and the worker reads it with `receiveMessageOnPort(parentPort)`, which is a
//! real Node API for exactly this. `parentPort.on('message', …)` exists on the
//! object (it is an `EventEmitter`) and never fires, because a worker here has
//! no event loop to fire it from: its thread runs the source to the end and
//! stops. That is a divergence and it is named rather than approximated.
//!
//! `worker.terminate()` sets a flag the worker sees where it polls. Stopping a
//! thread at an arbitrary instruction is not something this engine can do, so a
//! worker that never polls runs to its end.
//!
//! # Not implemented, by name
//!
//! `MessageChannel`/`MessagePort` between threads (the local pair below is
//! real and same-thread only), `BroadcastChannel`, `SHARE_ENV`, `transferList`
//! and every transfer, `markAsUntransferable`/`markAsUncloneable` and their
//! queries, `moveMessagePortToContext`, `postMessageToThread`, `locks`,
//! `resourceLimits` (an empty object, and nothing bounds a worker's heap),
//! `worker.stdin`/`stdout`/`stderr`, `getHeapSnapshot`, `getHeapStatistics`,
//! `cpuUsage`, `startCpuProfile`, `startHeapProfile`, `worker.performance`,
//! `ref`/`unref` (present and inert — the host joins every worker regardless).

mod portable;
mod registry;

/// This module as a loop source; see `registry::source`.
pub use registry::source;

use rts_core::entry::{self, Context, Provided};

use portable::{Portable, portable, rebuild};
use registry::{WorkerEvent, with_workers};



/// The namespace `node:worker_threads` is.
///
/// Built per thread, which is what makes `isMainThread` and `threadId` true
/// rather than remembered: `install` runs on whichever thread is starting, and
/// a worker's thread runs it too.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("receiveMessageOnPort", receive_message_on_port),
        ("isTerminating", is_terminating),
        ("getEnvironmentData", get_environment_data),
        ("setEnvironmentData", set_environment_data),
    ];
    let namespace = entry::make_namespace(context, members);
    entry::declare_loop_source(context, "node:worker_threads", source);

    // "worker_threads.Worker" rather than "Worker": `node:cluster` registers a
    // DIFFERENT class under that bare name (a forked OS process's handle, with
    // its own method table) — see the collision note on `cluster::namespace`
    // and on `make_prototype` itself.
    let prototype = entry::make_prototype(context, "worker_threads.Worker", WORKER_METHODS);
    let constructor = entry::make_callable(context, worker_ctor);
    entry::put_member(context, constructor, "prototype", prototype);
    entry::put_member(context, namespace, "Worker", constructor);

    let (main, id, name, data) = registry::side(|side| match side {
        None => (true, 0.0, None, Portable::Undefined),
        Some(side) => (
            false,
            side.id as f64,
            side.name.clone(),
            side.data.clone(),
        ),
    });
    let flag = entry::boolean_value(main);
    entry::put_member(context, namespace, "isMainThread", flag);
    // Always false: there are no engine-internal worker threads here, and the
    // reference doc says to document it as such rather than leave it absent.
    let internal = entry::boolean_value(false);
    entry::put_member(context, namespace, "isInternalThread", internal);
    let id = entry::make_number(id);
    entry::put_member(context, namespace, "threadId", id);
    let thread_name = match &name {
        Some(name) => entry::make_string(context, name),
        None => entry::null_in(context),
    };
    entry::put_member(context, namespace, "threadName", thread_name);
    let carried = rebuild(context, &data);
    entry::put_member(context, namespace, "workerData", carried);
    // Empty on both sides, because nothing here bounds a worker's heap. Node's
    // is empty on the main thread and populated in a worker; answering a made-up
    // number in a worker would be a claim about a limit that is not enforced.
    let limits = entry::make_object(context);
    entry::put_member(context, namespace, "resourceLimits", limits);
    let port = match main {
        true => entry::null_in(context),
        false => parent_port(context),
    };
    entry::put_member(context, namespace, "parentPort", port);
    namespace
}

/// The methods on `Worker.prototype`.
const WORKER_METHODS: &[(&str, Provided)] = &[
    ("postMessage", worker_post_message),
    ("terminate", worker_terminate),
    ("ref", inert),
    ("unref", inert),
];

/// The methods on the worker side's `parentPort`.
const PORT_METHODS: &[(&str, Provided)] = &[
    ("postMessage", port_post_message),
    ("close", inert),
    ("start", inert),
    ("ref", inert),
    ("unref", inert),
];

/// `new Worker(source[, options])`.
///
/// `options.eval` must be `true`; see the module doc for why a filename is
/// refused rather than resolved. The refusal arrives as `'error'` rather than a
/// throw, because a native here cannot throw one that a caller catches.
extern "C" fn worker_ctor(_e: u64, this: u64, source: u64, options: u64, _c: u64, _d: u64) -> u64 {
    // Everything read under ONE borrow, and the thread started after it is
    // dropped: starting a thread is not the risk, but `registry::start` takes
    // the evaluator off this thread's context, which is a second borrow.
    let (instance, text, evaluated, name, data) = entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "worker_threads.Worker", WORKER_METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        // What `EventEmitter` needs before anything can listen on it. `node:events`
        // is built before this module in `install`, so the prototype it chains onto
        // is the real one.
        let listeners = entry::make_object(context);
        entry::put_member(context, instance, "__events__", listeners);
        let emitter = entry::make_prototype(context, "EventEmitter", &[]);
        entry::set_prototype_in(context, prototype, emitter);

        let text = entry::text_in(context, source);
        let absent = entry::undefined_in(context);
        let evaluated = match options == absent {
            true => false,
            false => entry::get_member(context, options, "eval") == entry::boolean_value(true),
        };
        let name = match options == absent {
            true => None,
            false => {
                let held = entry::get_member(context, options, "name");
                entry::text_in(context, held)
            }
        };
        let data = match options == absent {
            true => Portable::Undefined,
            false => {
                let held = entry::get_member(context, options, "workerData");
                portable(context, held, 0)
            }
        };
        (instance, text, evaluated, name, data)
    });

    let refusal = match (text, evaluated) {
        (None, _) => Some("a Worker needs source text".to_owned()),
        (Some(_), false) => Some(
            "node:worker_threads here needs { eval: true }: a filename would have to be resolved \
             and this engine has no module loader"
                .to_owned(),
        ),
        (Some(source), true) => match registry::start(instance, source, name, data) {
            Some(id) => {
                let held = entry::with_runtime(|context| {
                    let held = entry::make_number(id as f64);
                    entry::put_member(context, instance, "threadId", held);
                    held
                });
                let _ = held;
                None
            }
            None => Some("no evaluator: this host cannot compile source".to_owned()),
        },
    };
    if let Some(reason) = refusal {
        // Queued rather than emitted here, and that is not laziness: a listener
        // is attached AFTER the constructor returns, so emitting now would reach
        // nobody. The table already exists for this exact ordering.
        let id = with_workers(|table| {
            let id = u64::MAX;
            table.insert(
                id,
                registry::WorkerEntry {
                    owner: std::thread::current().id(),
                    instance,
                    inbox: std::collections::VecDeque::new(),
                    outbox: std::sync::Arc::new(std::sync::Mutex::new(
                        std::collections::VecDeque::new(),
                    )),
                    stopping: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
                    finished: false,
                },
            );
            id
        });
        registry::deposit(id, WorkerEvent::Failed(reason));
        registry::deposit(id, WorkerEvent::Exited(1));
    }
    instance
}

/// The worker id an instance carries, or `None` when it never started.
fn worker_id(this: u64) -> Option<u64> {
    let held = entry::with_runtime(|context| entry::get_member(context, this, "threadId"));
    entry::number_of(held).map(|id| id as u64)
}

/// `worker.postMessage(value)` — queued for the worker to poll; see the module
/// doc's "the direction that is synchronous".
extern "C" fn worker_post_message(_e: u64, this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let carried = entry::with_runtime(|context| portable(context, value, 0));
    if let Some(id) = worker_id(this) {
        let outbox = with_workers(|table| table.get(&id).map(|entry| entry.outbox.clone()));
        if let Some(outbox) = outbox {
            let mut queue = outbox.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            queue.push_back(carried);
        }
    }
    entry::undefined_value()
}

/// `worker.terminate()` — sets the flag the worker reads where it polls.
///
/// Node answers a promise of the exit code. This answers `undefined`: a promise
/// resolved from a native is a mechanism this crate does not have, and the
/// `'exit'` event is the real signal either way.
extern "C" fn worker_terminate(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = worker_id(this) {
        with_workers(|table| {
            if let Some(entry) = table.get(&id) {
                entry
                    .stopping
                    .store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
    }
    entry::undefined_value()
}

/// The worker side's `parentPort`.
fn parent_port(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "MessagePort", PORT_METHODS);
    let port = entry::make_instance(context, prototype);
    let listeners = entry::make_object(context);
    entry::put_member(context, port, "__events__", listeners);
    let emitter = entry::make_prototype(context, "EventEmitter", &[]);
    entry::set_prototype_in(context, prototype, emitter);
    port
}

/// `parentPort.postMessage(value)` — worker → parent, delivered as `'message'`
/// on the parent's own thread.
extern "C" fn port_post_message(_e: u64, _this: u64, value: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let carried = entry::with_runtime(|context| portable(context, value, 0));
    let id = registry::side(|side| side.map(|side| side.id));
    if let Some(id) = id {
        registry::deposit(id, WorkerEvent::Message(carried));
    }
    entry::undefined_value()
}

/// `receiveMessageOnPort(port)` — one queued message as `{ message }`, or
/// `undefined`.
///
/// On the worker's side this is how a parent's `postMessage` is read at all,
/// and it is also where `terminate()` is observed: a worker polling an empty
/// queue after being terminated gets `undefined` forever, which is the signal a
/// loop reads to stop.
extern "C" fn receive_message_on_port(_e: u64, _this: u64, _p: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let taken = registry::side(|side| {
        let side = side?;
        let mut queue = side
            .outbox
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        queue.pop_front()
    });
    entry::with_runtime(|context| match taken {
        None => entry::undefined_in(context),
        Some(value) => {
            let held = rebuild(context, &value);
            let wrapper = entry::make_object(context);
            entry::put_member(context, wrapper, "message", held);
            wrapper
        }
    })
}

/// `getEnvironmentData(key)` / `setEnvironmentData(key, value)`.
///
/// A process-wide table of portable values, which is what the API is: it is
/// read by workers started AFTER the write, and it is the one piece of state
/// here that deliberately crosses threads.
static ENVIRONMENT: Mutex<Option<HashMap<String, Portable>>> = Mutex::new(None);

use std::collections::HashMap;
use std::sync::Mutex;

fn with_environment<T>(body: impl FnOnce(&mut HashMap<String, Portable>) -> T) -> T {
    let mut guard = ENVIRONMENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

extern "C" fn get_environment_data(_e: u64, _this: u64, key: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let name = entry::text_of(key);
    entry::with_runtime(|context| {
        let held = name.and_then(|name| with_environment(|table| table.get(&name).cloned()));
        match held {
            Some(value) => rebuild(context, &value),
            None => entry::undefined_in(context),
        }
    })
}

extern "C" fn set_environment_data(_e: u64, _this: u64, key: u64, value: u64, _c: u64, _d: u64) -> u64 {
    let (name, carried) = entry::with_runtime(|context| {
        (entry::text_in(context, key), portable(context, value, 0))
    });
    if let Some(name) = name {
        with_environment(|table| table.insert(name, carried));
    }
    entry::undefined_value()
}

/// Present and does nothing, so a program calling it is not stopped by its
/// absence. `ref`/`unref` are the members this is: the host joins every worker
/// regardless of them.
extern "C" fn inert(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}

/// `isTerminating()` — whether `worker.terminate()` was called on this thread.
///
/// # NOT a Node API, and why it exists anyway
///
/// In Node, `terminate()` kills the thread and a worker never observes it.
/// Nothing here can stop a thread at an arbitrary instruction, so the choice was
/// between a `terminate()` that does nothing at all and one a cooperative worker
/// can honour. This is the second, under a name a program can only reach
/// deliberately — inventing a Node-shaped name for a non-Node capability is how
/// a divergence stops being visible.
///
/// `false` on the main thread, which is also what a `Worker` that was never
/// terminated answers.
extern "C" fn is_terminating(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let stopping = registry::side(|side| {
        side.is_some_and(|side| side.stopping.load(std::sync::atomic::Ordering::SeqCst))
    });
    entry::boolean_value(stopping)
}
