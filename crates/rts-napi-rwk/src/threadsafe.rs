//! Calling a JavaScript function from a thread that has no JavaScript.
//!
//! P7b, and the mirror of [`crate::async_work`]: there, work leaves the
//! JavaScript thread and a result comes back; here, an arbitrary thread asks
//! for a call and the JavaScript thread makes it.
//!
//! # What crosses, and what does not
//!
//! A `void*` the addon owns. That is all. `napi_call_threadsafe_function` takes
//! no `napi_value` and returns none — the ABI's own shape — so a worker never
//! holds an engine value and never needs a `Context`. The JavaScript function
//! itself stays on the JavaScript thread, held there by a strong external root
//! (P4's mechanism) so that a collection between calls cannot take it.
//!
//! That is what makes this implementable on an engine with one JavaScript
//! thread, and why it does not contradict `CLAUDE.md`'s `thread` entry: nothing
//! here runs JavaScript anywhere new.
//!
//! # Two halves, and why the split is where it is
//!
//! What the addon holds is a pointer to [`Shared`], which is `Send` and carries
//! only a channel and counters. Everything the engine touches — the receiver,
//! the rooted function, the callback that builds handles — lives in a
//! thread-local slot on the JavaScript thread, and the two halves find each
//! other through the `Arc` they share. A single struct holding both would have
//! to be `Send` as a whole, which would be a lie about the half that is not.
//!
//! # The reference count is two counts
//!
//! The ABI has thread acquisition (`acquire`/`release`, how many threads may
//! still call) and loop referencing (`ref`/`unref`, whether an idle queue keeps
//! the program alive). They are independent and are two fields here for that
//! reason: an addon commonly `unref`s a long-lived function so its existence
//! does not stop the process, while several threads still hold it.

use core::cell::RefCell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of};

use napi_status::{napi_closing, napi_invalid_arg, napi_ok};

/// What the ABI hands an addon. Opaque, and valid on any thread.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct napi_threadsafe_function(pub *mut c_void);

/// What `napi_release_threadsafe_function` is asked to do.
///
/// **The order is the ABI.**
#[allow(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum napi_threadsafe_function_release_mode {
    napi_tsfn_release = 0,
    napi_tsfn_abort,
}

/// What `napi_call_threadsafe_function` is asked to do when the queue is full.
///
/// **The order is the ABI.**
#[allow(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum napi_threadsafe_function_call_mode {
    napi_tsfn_nonblocking = 0,
    napi_tsfn_blocking,
}

/// What the addon's `call_js_cb` looks like.
pub type CallJs = Option<
    unsafe extern "C" fn(
        env: napi_env,
        js_callback: napi_value,
        context: *mut c_void,
        data: *mut c_void,
    ),
>;

/// A `void*` on its way from a worker to the JavaScript thread.
struct Item(*mut c_void);

// SAFETY: the pointer is the addon's and is handed over wholesale — the ABI's
// contract is that whoever calls `napi_call_threadsafe_function` gives up the
// data until `call_js_cb` receives it. Nothing of the engine's is inside.
unsafe impl Send for Item {}

/// The half that may be touched from any thread.
struct Shared {
    to: Sender<Item>,
    /// How many threads may still call. At zero the function is finished.
    threads: AtomicUsize,
    /// Whether an idle queue keeps the program alive.
    referenced: AtomicBool,
    /// Set by `abort`, and by the last release. Refuses further calls.
    closing: AtomicBool,
    /// The addon's own context pointer, handed back on every call.
    ///
    /// Read-only after creation, which is what makes sharing it across threads
    /// sound — the ABI describes it the same way.
    context: usize,
}

/// The half that only the JavaScript thread may touch.
struct Owned {
    from: Receiver<Item>,
    /// The external root keeping the JavaScript function alive between calls.
    held: u32,
    call_js: CallJs,
    owner: *mut c_void,
    shared: Arc<Shared>,
}

thread_local! {
    /// Every threadsafe function this JavaScript thread created, by slot.
    ///
    /// Holes rather than a compacting list: the slot number is what the shared
    /// half carries across threads, so removing by shifting would renumber a
    /// function another thread is already holding.
    static OWNED: RefCell<Vec<Option<Owned>>> = const { RefCell::new(Vec::new()) };
}

/// How long the loop waits before asking again while a call may still arrive.
///
/// A poll, for the reason [`crate::async_work`] states at length: `entry::loops`
/// has no "asleep until woken", and its `Blocked` does not hold a program open.
const POLL: core::time::Duration = core::time::Duration::from_millis(1);

/// Makes whatever calls have been queued, and says whether more may come.
fn deliver() -> rts_core::entry::Pending {
    loop {
        // One item at a time, with nothing borrowed while the callback runs:
        // it is user code and may create, release, or call this very function.
        let next = OWNED.with_borrow(|owned| {
            owned.iter().enumerate().find_map(|(slot, entry)| {
                let entry = entry.as_ref()?;
                let Item(data) = entry.from.try_recv().ok()?;
                Some((slot, data))
            })
        });
        let Some((slot, data)) = next else {
            break;
        };
        let Some((call_js, owner, held, context)) = OWNED.with_borrow(|owned| {
            let entry = owned.get(slot)?.as_ref()?;
            Some((
                entry.call_js,
                entry.owner,
                entry.held,
                entry.shared.context,
            ))
        }) else {
            continue;
        };

        let word = rts_core::entry::held_current(held);
        // SAFETY: the owner pointer came from `Env::into_raw` and the
        // environment outlives every function it created.
        let Some(env) = (unsafe { env_of(napi_env(owner)) }) else {
            continue;
        };
        // The call gets a scope of its own, closed after it, so a program that
        // takes a million calls does not accumulate a million roots.
        env.open();
        let handle = match word {
            Some(word) => env.current().handle(word),
            None => crate::handles::none(),
        };
        if let Some(call_js) = call_js {
            // SAFETY: the addon's own function, on the JavaScript thread, with
            // its own context and its own data. Where the ABI says it runs.
            unsafe { call_js(napi_env(owner), handle, context as *mut c_void, data) };
        }
        // SAFETY: re-derived rather than held across the call, which may have
        // created another threadsafe function and grown the table.
        if let Some(env) = unsafe { env_of(napi_env(owner)) } {
            env.close();
        }
    }

    // Alive if any function on this thread still has a thread holding it AND is
    // referenced. `unref` is exactly how an addon says "do not keep the process
    // open for this", and honouring it is the difference between a program that
    // exits and one that hangs.
    let waiting = OWNED.with_borrow(|owned| {
        owned.iter().flatten().any(|entry| {
            entry.shared.referenced.load(Ordering::SeqCst)
                && entry.shared.threads.load(Ordering::SeqCst) > 0
                && !entry.shared.closing.load(Ordering::SeqCst)
        })
    });
    match waiting {
        true => rts_core::entry::Pending::In(POLL),
        false => rts_core::entry::Pending::Idle,
    }
}

/// The shared half a handle names.
///
/// # Safety
///
/// `func` must be one [`napi_create_threadsafe_function`] produced and not yet
/// finished.
unsafe fn shared_of(func: napi_threadsafe_function) -> Option<Arc<Shared>> {
    if func.0.is_null() {
        return None;
    }
    // SAFETY: the caller's contract. Cloned rather than borrowed so the caller
    // holds an owner while it works, which is what makes a concurrent release
    // safe.
    let shared = unsafe { &*func.0.cast::<Arc<Shared>>() };
    Some(Arc::clone(shared))
}

/// `napi_create_threadsafe_function`.
///
/// # Safety
///
/// The ABI's. Must be called on the JavaScript thread.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn napi_create_threadsafe_function(
    env: napi_env,
    func: napi_value,
    _async_resource: napi_value,
    _async_resource_name: napi_value,
    _max_queue_size: usize,
    initial_thread_count: usize,
    _thread_finalize_data: *mut c_void,
    _thread_finalize_cb: crate::abi::napi_finalize,
    context: *mut c_void,
    call_js_cb: CallJs,
    result: *mut napi_threadsafe_function,
) -> napi_status {
    if initial_thread_count == 0 || result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(func) }) else {
        return napi_invalid_arg;
    };
    if !rts_core::entry::with_runtime(|runtime| rts_core::entry::is_callable_in(runtime, word)) {
        return napi_status::napi_function_expected;
    }

    let (to, from) = channel();
    let slot = OWNED.with_borrow(|owned| {
        owned
            .iter()
            .position(Option::is_none)
            .unwrap_or(owned.len())
    });
    let shared = Arc::new(Shared {
        to,
        threads: AtomicUsize::new(initial_thread_count),
        referenced: AtomicBool::new(true),
        closing: AtomicBool::new(false),
        context: context as usize,
    });
    // The function is rooted for as long as the threadsafe function lives: a
    // worker may call it three turns from now, and nothing else is keeping it.
    let held = rts_core::entry::hold_current(word);
    let entry = Owned {
        from,
        held,
        call_js: call_js_cb,
        owner: env.0,
        shared: Arc::clone(&shared),
    };
    OWNED.with_borrow_mut(|owned| match owned.len() > slot {
        true => owned[slot] = Some(entry),
        false => owned.push(Some(entry)),
    });
    rts_core::entry::with_runtime(|runtime| {
        rts_core::entry::declare_loop_source(runtime, "napi:threadsafe", deliver)
    });

    // SAFETY: the caller's contract — `result` writable.
    unsafe { *result = napi_threadsafe_function(Box::into_raw(Box::new(shared)).cast()) };
    napi_ok
}

/// `napi_call_threadsafe_function` — from any thread.
///
/// # Safety
///
/// `func` must be live, and `data` must stay valid until `call_js_cb` has
/// received it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_call_threadsafe_function(
    func: napi_threadsafe_function,
    data: *mut c_void,
    _is_blocking: napi_threadsafe_function_call_mode,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(shared) = (unsafe { shared_of(func) }) else {
        return napi_invalid_arg;
    };
    if shared.closing.load(Ordering::SeqCst) {
        return napi_closing;
    }
    // The blocking mode is not honoured and cannot be misread as honoured,
    // because there is no bound to block against: `max_queue_size` is ignored
    // and the channel is unbounded, so a call never has to wait. If a bound
    // arrives, this is where blocking becomes a real question.
    match shared.to.send(Item(data)) {
        Ok(()) => napi_ok,
        // The receiver is gone, which means the JavaScript thread finished.
        Err(_) => napi_closing,
    }
}

/// `napi_acquire_threadsafe_function` — one more thread may call.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_acquire_threadsafe_function(
    func: napi_threadsafe_function,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(shared) = (unsafe { shared_of(func) }) else {
        return napi_invalid_arg;
    };
    if shared.closing.load(Ordering::SeqCst) {
        return napi_closing;
    }
    shared.threads.fetch_add(1, Ordering::SeqCst);
    napi_ok
}

/// `napi_release_threadsafe_function` — this thread is done with it.
///
/// # Safety
///
/// The caller must not use `func` again after releasing its own acquisition.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_release_threadsafe_function(
    func: napi_threadsafe_function,
    mode: napi_threadsafe_function_release_mode,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(shared) = (unsafe { shared_of(func) }) else {
        return napi_invalid_arg;
    };
    if mode == napi_threadsafe_function_release_mode::napi_tsfn_abort {
        shared.closing.store(true, Ordering::SeqCst);
    }
    let before = shared.threads.fetch_sub(1, Ordering::SeqCst);
    if before <= 1 {
        // The last thread. Nothing may call again, and the JavaScript half is
        // released on the next pass of the loop — not here, because this may be
        // running on a worker and the root belongs to the other thread.
        shared.closing.store(true, Ordering::SeqCst);
    }
    napi_ok
}

/// `napi_ref_threadsafe_function` — keep the program alive for this.
///
/// # Safety
///
/// The ABI's. Must be called on the JavaScript thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_ref_threadsafe_function(
    _env: napi_env,
    func: napi_threadsafe_function,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(shared) = (unsafe { shared_of(func) }) else {
        return napi_invalid_arg;
    };
    shared.referenced.store(true, Ordering::SeqCst);
    napi_ok
}

/// `napi_unref_threadsafe_function` — stop keeping the program alive.
///
/// # Safety
///
/// The ABI's. Must be called on the JavaScript thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_unref_threadsafe_function(
    _env: napi_env,
    func: napi_threadsafe_function,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(shared) = (unsafe { shared_of(func) }) else {
        return napi_invalid_arg;
    };
    shared.referenced.store(false, Ordering::SeqCst);
    napi_ok
}

/// `napi_get_threadsafe_function_context`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_threadsafe_function_context(
    func: napi_threadsafe_function,
    result: *mut *mut c_void,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(shared) = (unsafe { shared_of(func) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = shared.context as *mut c_void };
    napi_ok
}

/// Releases every threadsafe function an environment made.
///
/// Called from [`crate::env::destroy`]. The root goes back here rather than
/// when the last thread releases, because that release may happen on a worker
/// and a root belongs to the thread that took it.
pub fn forget(owner: napi_env) {
    let mine: Vec<Owned> = OWNED.with_borrow_mut(|owned| {
        let mut mine = Vec::new();
        for slot in owned.iter_mut() {
            if slot.as_ref().is_some_and(|entry| entry.owner == owner.0)
                && let Some(entry) = slot.take()
            {
                mine.push(entry);
            }
        }
        mine
    });
    for entry in mine {
        entry.shared.closing.store(true, Ordering::SeqCst);
        rts_core::entry::release_current(entry.held);
    }
}
