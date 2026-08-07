//! The thread a `Worker` is, and the handoff between it and the thread that
//! made it.
//!
//! # What makes a second thread possible at all
//!
//! A context is thread-local and a region belongs to the thread that installed
//! it — so a worker cannot be a callback on a shared heap, and this is not the
//! `fs.watch` shape where a background thread only reads bytes. A worker RUNS
//! JavaScript, which needs a heap, which means a whole engine.
//!
//! `entry::evaluator()` is what makes that reachable: the host's evaluator is a
//! `fn` pointer, so it is `Copy` and `Send`, and the far thread calls it and
//! gets its own context, its own region and its own installed modules out of
//! the host's own compile path. Nothing is shared, which is why nothing needs a
//! lock around it.
//!
//! # The delivery rule, unchanged
//!
//! A worker thread never calls a listener. It pushes a native record — a
//! [`Portable`], which holds no cell — into the table here, and [`pump`] turns
//! that into a call on the parent's thread. Same recipe as `fs/watch.rs`,
//! `dgram` and `net`, and for the same reason: calling into JS from a foreign
//! thread aborts on the first event.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rts_core_rwk::entry;

use super::portable::{Portable, rebuild};

/// One thing a worker's thread observed, in native data only.
pub(super) enum WorkerEvent {
    /// `parentPort.postMessage(value)` on the worker's side.
    Message(Portable),
    /// The worker's source did not compile, or the host has no evaluator.
    Failed(String),
    /// The worker's source ran to its end.
    Exited(i32),
}

/// What the parent holds about one worker.
pub(super) struct WorkerEntry {
    /// The thread that started it.
    ///
    /// The table is process-wide and every thread running a program reaches it,
    /// including a worker's own — so without this a worker `join_all`s the
    /// entry for ITSELF and waits for a thread that is waiting on it, and its
    /// `pump` emits onto a JS instance that belongs to the parent's region.
    /// Both were live: the first hung the suite, the second is worse for being
    /// silent.
    pub(super) owner: std::thread::ThreadId,
    /// The `Worker` JS instance, in the PARENT's region — which is why nothing
    /// on the worker's thread may touch it.
    pub(super) instance: u64,
    /// Worker → parent, drained by [`pump`].
    pub(super) inbox: VecDeque<WorkerEvent>,
    /// Parent → worker. Shared with the worker's thread, which is why it is an
    /// `Arc<Mutex<_>>` of its own rather than a field read under the table lock:
    /// the worker polls it while the parent is running, and taking the table
    /// lock from over there would serialise the two threads.
    pub(super) outbox: Arc<Mutex<VecDeque<Portable>>>,
    /// Set by `worker.terminate()`. The worker reads it where it polls, so a
    /// worker that never polls runs to its end — stopping a thread at an
    /// arbitrary instruction is not something this engine can do, and the module
    /// doc says so rather than implying a kill.
    pub(super) stopping: Arc<AtomicBool>,
    pub(super) finished: bool,
}

static WORKERS: Mutex<Option<HashMap<u64, WorkerEntry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Which worker THIS thread is, absent on the main thread.
    ///
    /// This is what makes `isMainThread`, `threadId`, `workerData` and
    /// `parentPort` answerable: `namespace` runs on whichever thread installed
    /// the modules, so it reads its answers from here rather than from a flag
    /// something has to remember to pass.
    static SELF: std::cell::RefCell<Option<Side>> = const { std::cell::RefCell::new(None) };
}

/// What a worker's own thread knows about itself.
pub(super) struct Side {
    pub(super) id: u64,
    pub(super) name: Option<String>,
    pub(super) data: Portable,
    pub(super) outbox: Arc<Mutex<VecDeque<Portable>>>,
    pub(super) stopping: Arc<AtomicBool>,
}

pub(super) fn with_workers<T>(body: impl FnOnce(&mut HashMap<u64, WorkerEntry>) -> T) -> T {
    let mut guard = WORKERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// What this thread is, for the module properties that differ per thread.
pub(super) fn side<T>(body: impl FnOnce(Option<&Side>) -> T) -> T {
    SELF.with(|slot| body(slot.borrow().as_ref()))
}

/// Starts a worker over `source`, answering its thread id.
///
/// `None` when no host installed an evaluator — a worker cannot be started
/// without one, and answering that is what lets the module say so instead of
/// spawning a thread that immediately fails.
pub(super) fn start(
    instance: u64,
    source: String,
    name: Option<String>,
    data: Portable,
) -> Option<u64> {
    // Taken HERE, on the parent's thread, because it is read off this thread's
    // context — the worker's thread has none until it runs, so asking over there
    // would abort before anything else could go wrong.
    let evaluator = entry::evaluator()?;
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let outbox = Arc::new(Mutex::new(VecDeque::new()));
    let stopping = Arc::new(AtomicBool::new(false));
    with_workers(|table| {
        table.insert(
            id,
            WorkerEntry {
                owner: std::thread::current().id(),
                instance,
                inbox: VecDeque::new(),
                outbox: Arc::clone(&outbox),
                stopping: Arc::clone(&stopping),
                finished: false,
            },
        );
    });
    let side = Side {
        id,
        name,
        data,
        outbox,
        stopping,
    };
    let started = std::thread::Builder::new()
        .name(format!("rts-worker-{id}"))
        .spawn(move || {
            SELF.with(|slot| *slot.borrow_mut() = Some(side));
            // The evaluator compiles and runs, installing a context, a region
            // and every module over there. Its answer is deliberately dropped:
            // a reference belongs to the region that made it, and that region
            // goes away with this thread.
            match evaluator(&source) {
                Some(_) | None => {}
            }
            deposit(id, WorkerEvent::Exited(0));
        });
    match started {
        Ok(_) => Some(id),
        Err(error) => {
            deposit(id, WorkerEvent::Failed(error.to_string()));
            Some(id)
        }
    }
}

/// Queues something for the parent to deliver.
pub(super) fn deposit(id: u64, event: WorkerEvent) {
    with_workers(|table| {
        if let Some(entry) = table.get_mut(&id) {
            entry.inbox.push_back(event);
        }
    });
}

/// Delivers every queued event, oldest first per worker, with the table lock
/// released before anything is called.
///
/// A listener may call back into `node:worker_threads` — posting a reply from
/// inside `'message'` is the ordinary case — and holding the lock across the
/// call would deadlock on the first one.
pub(super) fn pump() {
    let due: Vec<(u64, Vec<WorkerEvent>)> = with_workers(|table| {
        table
            .iter_mut()
            .filter(|(_, entry)| entry.owner == std::thread::current().id())
            .filter(|(_, entry)| !entry.inbox.is_empty())
            .map(|(&id, entry)| (id, entry.inbox.drain(..).collect()))
            .collect()
    });
    for (id, events) in due {
        let Some(instance) = with_workers(|table| table.get(&id).map(|entry| entry.instance)) else {
            continue;
        };
        for event in events {
            match event {
                WorkerEvent::Message(value) => {
                    let carried = entry::with_runtime(|context| rebuild(context, &value));
                    emit(instance, "message", carried);
                }
                WorkerEvent::Failed(reason) => {
                    let error = entry::with_runtime(|context| {
                        let object = entry::make_object(context);
                        let message = entry::make_string(context, &reason);
                        entry::put_member(context, object, "message", message);
                        object
                    });
                    emit(instance, "error", error);
                }
                WorkerEvent::Exited(code) => {
                    with_workers(|table| {
                        if let Some(entry) = table.get_mut(&id) {
                            entry.finished = true;
                        }
                    });
                    let code = entry::make_number(f64::from(code));
                    emit(instance, "exit", code);
                }
            }
        }
    }
}

/// Waits for every worker to finish, delivering what they queued.
///
/// # Why the host calls this and a program does not
///
/// Node keeps a process alive while a worker runs, and this engine's host
/// otherwise returns the moment the main program's last statement does — so a
/// worker's `'message'` would be queued into a table nothing ever reads again.
/// Joining here is what makes `worker.on('message', …)` mean something without
/// an event loop to hold the process open.
///
/// Pumping runs BETWEEN joins as well as after: a worker that posts and then
/// keeps running has its message delivered while it still runs, which is what a
/// program expects of a message.
pub fn join_all() {
    loop {
        pump();
        let outstanding = with_workers(|table| {
            table
                .values()
                .filter(|entry| entry.owner == std::thread::current().id())
                .filter(|entry| !entry.finished)
                .count()
        });
        if outstanding == 0 {
            return;
        }
        // Sleeping rather than spinning: a bare `yield_now` loop burns a core
        // for the length of the worker, and a worker is the thing most likely
        // to want that core. Two hundred microseconds is below what a message
        // round trip is worth measuring at.
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
}

/// Calls `instance.emit(event, value)`, looked up fresh and never while a
/// runtime borrow is held — the recipe `dgram` and `net` already use, and for
/// the same reason: the listener is user code and it will call back in.
fn emit(instance: u64, event: &str, value: u64) {
    let emitter = entry::with_runtime(|context| entry::get_member(context, instance, "emit"));
    let absent = entry::undefined_value();
    if emitter == absent {
        return;
    }
    let name = entry::with_runtime(|context| entry::make_string(context, event));
    entry::call(emitter, instance, name, value, absent, absent);
}
