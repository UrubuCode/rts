//! Work that runs off the JavaScript thread, and comes back to it.
//!
//! P7. `napi_create_async_work` + `napi_queue_async_work`: an addon hands over
//! an `execute` and a `complete`, the first runs somewhere else, the second
//! runs where JavaScript lives.
//!
//! # Why this is possible on an engine with one JavaScript thread
//!
//! Because the ABI already draws the line where this engine needs it. An
//! `execute` callback **may not call any `napi_*` function** — that is Node's
//! own contract, not a limitation invented here — so it never wants a
//! `Context`, and a `Context` never has to cross a thread. What crosses is the
//! addon's own `data` pointer, which the addon owns on both sides.
//!
//! `complete` does touch JavaScript, and it runs on the JavaScript thread. So
//! the shape is: spawn, work, post back, deliver at the loop.
//!
//! This is what `CLAUDE.md`'s `thread` entry rules out and this does not
//! contradict it. That entry is about running JAVASCRIPT on two threads, which
//! would need two `Context`s and a way to publish a value between them. Nothing
//! here does that: one thread runs JavaScript, exactly as before.
//!
//! # Why delivery is a loop source and not a drain
//!
//! Because completion must also keep the program ALIVE. A queued piece of work
//! is an outstanding reason not to exit, the same way a pending timer is, and
//! `entry::loops` is where this engine says that. A finalizer drain could not:
//! it runs when something else already decided to keep going.
//!
//! # What the old engine did here, and why this is not that
//!
//! `rts-napi`'s version ran `execute` and `complete` back to back on the
//! calling thread and said so in its module doc — "não há paralelismo real (a
//! chamada async bloqueia até completar)". It was honest and it was the right
//! call then, because there was no loop to post back to. There is one now.

use core::cell::{Cell, RefCell};
use core::ffi::c_void;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::abi::{napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

/// What runs off-thread. Handed the addon's `data` and nothing else.
pub type ExecuteCb = Option<unsafe extern "C" fn(env: napi_env, data: *mut c_void)>;

/// What runs back on the JavaScript thread, with how the work ended.
pub type CompleteCb =
    Option<unsafe extern "C" fn(env: napi_env, status: napi_status, data: *mut c_void)>;

/// One piece of work, from creation until its `complete` has run.
struct Work {
    execute: ExecuteCb,
    complete: CompleteCb,
    data: *mut c_void,
    owner: *mut c_void,
    /// Whether it has been queued, so a second queue is refused rather than
    /// running the addon's `execute` twice against one `data`.
    queued: bool,
}

/// A `Work` on its way to a worker thread and back.
///
/// The pointers are the addon's and are moved wholesale between two threads,
/// which is exactly what the ABI describes: the addon owns `data` and promises
/// not to touch it while the work is outstanding. Rust cannot check that
/// promise, which is what this wrapper says out loud.
struct Crossing(Work);

// SAFETY: what crosses is the addon's own `data` pointer plus two function
// pointers, and the ABI's contract is that `data` belongs to the work while it
// is outstanding — the addon may not read it from another thread until
// `complete` runs. No engine value crosses: `execute` may not call `napi_*`,
// which is the ABI's rule and the reason this is soundly implementable at all.
unsafe impl Send for Crossing {}

/// The finished work waiting for the JavaScript thread to notice.
///
/// A channel rather than a mutex-and-vector because the producer is a worker
/// thread and the consumer is the loop: a queue with a wakeup is exactly what a
/// channel is, and rebuilding one out of a `Mutex` would be the second copy
/// this repository's rules keep naming.
struct Done {
    to: Sender<Crossing>,
    from: Receiver<Crossing>,
}

thread_local! {
    /// The channel this JavaScript thread's workers post back to.
    ///
    /// Thread-local like every other table in this crate, and the reason is
    /// worth stating because a process-global one is the obvious first
    /// implementation and it is WRONG: two JavaScript threads — two regions,
    /// which `rts-host::compile_for` supports — would share one queue, and
    /// whichever pumped first would deliver the other's completions on the
    /// wrong thread, running `complete` against a `Context` that never made
    /// those values.
    ///
    /// It was also wrong in a way that showed up immediately: cargo runs the
    /// tests of one binary on several threads, and with a global queue they
    /// stole each other's work.
    ///
    /// The worker does not read this. It is handed a CLONE of the sender when
    /// it is spawned, which is the only thing that crosses.
    static QUEUE: RefCell<Option<Done>> = const { RefCell::new(None) };

    /// How many pieces of work this thread has queued and not yet delivered.
    ///
    /// A count rather than asking the channel, because an empty channel means
    /// "nothing ready", not "nothing outstanding" — and exiting on the first is
    /// how a program ends before its own callback runs.
    static OUTSTANDING: Cell<usize> = const { Cell::new(0) };
}

/// How long the loop waits before asking again while work is outstanding.
///
/// A POLL, and it is one because `entry::loops` has no wakeup: its two answers
/// are a deadline and `Blocked`, and `Blocked` explicitly "does NOT hold the
/// program open" — which for this source would mean exiting while a worker is
/// still running, before the addon's own callback. So the honest answer is a
/// deadline, and the honest deadline for "a thread finishes when it finishes"
/// is a short one.
///
/// The cost is one pass of the loop per millisecond while an addon has work
/// outstanding, and nothing at all otherwise: the source answers `Idle` and is
/// not asked again until something is queued. What removes the poll is a
/// third answer from `entry::loops` — "asleep until woken" — which is a change
/// to the loop and not to this file.
const POLL: core::time::Duration = core::time::Duration::from_millis(1);

/// Delivers whatever finished, and says whether more is coming.
///
/// Registered with `entry::loops` the first time work is queued.
fn deliver() -> rts_core::entry::Pending {
    loop {
        // Taken with the borrow given straight back: `complete` runs user code
        // and may queue more work, which would borrow this again.
        let finished = QUEUE.with_borrow(|queue| match queue.as_ref() {
            Some(done) => done.from.try_recv().ok(),
            None => None,
        });
        let Some(Crossing(work)) = finished else {
            break;
        };
        // The count drops BEFORE the callback runs: `complete` may queue more
        // work, and a count still holding this one would make the loop think
        // two things are outstanding when one is.
        OUTSTANDING.with(|outstanding| outstanding.set(outstanding.get().saturating_sub(1)));
        if let Some(complete) = work.complete {
            // SAFETY: the addon's own function, on the JavaScript thread, with
            // the environment it registered under. That is where the ABI says
            // `complete` runs and the only place it may touch JavaScript.
            unsafe { complete(napi_env(work.owner), napi_ok, work.data) };
        }
    }

    match OUTSTANDING.with(Cell::get) {
        0 => rts_core::entry::Pending::Idle,
        _ => rts_core::entry::Pending::In(POLL),
    }
}

/// `napi_create_async_work`.
///
/// # Safety
///
/// The ABI's: `execute` must not call any `napi_*` function, and `data` must
/// stay valid until `complete` has run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_async_work(
    env: napi_env,
    _async_resource: napi_value,
    _async_resource_name: napi_value,
    execute: ExecuteCb,
    complete: CompleteCb,
    data: *mut c_void,
    result: *mut *mut c_void,
) -> napi_status {
    if execute.is_none() || result.is_null() {
        return napi_invalid_arg;
    }
    let work = Box::into_raw(Box::new(Work {
        execute,
        complete,
        data,
        owner: env.0,
        queued: false,
    }));
    // SAFETY: the caller's contract — `result` writable.
    unsafe { *result = work.cast() };
    napi_ok
}

/// `napi_queue_async_work` — run it, off this thread.
///
/// # Safety
///
/// `work` must be one [`napi_create_async_work`] produced and not yet deleted.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_queue_async_work(
    _env: napi_env,
    work: *mut c_void,
) -> napi_status {
    if work.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let Some(held) = (unsafe { work.cast::<Work>().as_mut() }) else {
        return napi_invalid_arg;
    };
    // Refused rather than run twice: a second queue against one `data` is the
    // addon's bug, and running it would have two threads on one pointer.
    if held.queued {
        return napi_invalid_arg;
    }
    held.queued = true;

    let sender = QUEUE.with_borrow_mut(|queue| {
        let done = queue.get_or_insert_with(|| {
            let (to, from) = channel();
            Done { to, from }
        });
        done.to.clone()
    });
    OUTSTANDING.with(|outstanding| outstanding.set(outstanding.get() + 1));
    // Registered on first use rather than at startup: a program with no addon
    // should not carry a source that always answers `Idle`.
    rts_core::entry::with_runtime(|context| {
        rts_core::entry::declare_loop_source(context, "napi:async_work", deliver)
    });

    // SAFETY: the box came from `napi_create_async_work` and this is the only
    // place that takes it back; the work is now owned by the thread below until
    // it is posted back and delivered.
    let crossing = Crossing(*unsafe { Box::from_raw(work.cast::<Work>()) });
    std::thread::Builder::new()
        .name("napi-async-work".to_owned())
        .spawn(move || {
            let crossing = crossing;
            if let Some(execute) = crossing.0.execute {
                // SAFETY: the addon's own function, off the JavaScript thread,
                // handed only its own `data`. The ABI forbids it from calling
                // back into `napi_*`, which is what makes this sound.
                unsafe { execute(napi_env(crossing.0.owner), crossing.0.data) };
            }
            // A send that fails means the receiver is gone, which means the
            // program is tearing down. Dropping the work is the only thing left
            // to do and is better than a panic on a thread nobody joins.
            let _ = sender.send(crossing);
        })
        .map(|_| napi_ok)
        .unwrap_or(napi_status::napi_generic_failure)
}

/// `napi_delete_async_work` — for work that was created and never queued.
///
/// Queued work is owned by the thread running it and freed when its `complete`
/// has run, so deleting it here would be freeing a pointer another thread is
/// using. The ABI says the same: delete after `complete`, or before `queue`.
///
/// # Safety
///
/// `work` must not have been queued, and must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_async_work(
    _env: napi_env,
    work: *mut c_void,
) -> napi_status {
    if work.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let held = unsafe { Box::from_raw(work.cast::<Work>()) };
    match held.queued {
        // Already handed to a thread. Putting the box back is the least wrong
        // thing available — leaking one record beats freeing memory a worker is
        // reading.
        true => {
            Box::into_raw(held);
            napi_invalid_arg
        }
        false => napi_ok,
    }
}
