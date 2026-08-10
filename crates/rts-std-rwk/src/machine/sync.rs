//! `rts`'s `sync` — a mutex over one `i64` cell, addressed by handle.
//!
//! # What this engine can honestly call a mutex
//!
//! A mutex is a promise: at most one holder at a time, and a second one that
//! asks BLOCKS until the first lets go. This engine cannot keep that promise
//! yet — one OS thread runs JavaScript (see `atomic.rs`'s module doc for the
//! evidence: no code in `crates/rts-host-rwk` starts a second one to run a
//! callback), so `mutex_lock` never has a second asker to block, and always
//! succeeds immediately. That is not "a mutex that never contends" wearing the
//! name for convenience — it is stated here so nobody reads `mutex_lock`
//! blocking as tested. **The day a second thread can call in, this is the file
//! that has to grow a real `std::sync::Mutex` underneath, and until then this
//! is a single-holder cell that answers the single-threaded fixtures
//! correctly rather than a lock that has been exercised under contention.**
//!
//! # `mutex_free` while locked (issue #280, in the old engine)
//!
//! The old engine's bug was UB from freeing a `Box` while a `'static`
//! `MutexGuard` still pointed into it. There is no such guard here: `lock`
//! answers the stored value itself, not a handle to borrowed memory, so
//! freeing the cell afterward frees a `Vec` slot and nothing else can be
//! holding a reference into it. `mutex_free` while "locked" is consequently
//! not a hazard this design can reproduce — the flag is bookkeeping for
//! `unlock` to clear, not a borrow anything outlives.

use std::cell::RefCell;

use rts_core_rwk::entry::{self, Context, Provided};

/// The `sync` namespace.
pub fn install(context: &mut Context, surface: u64) {
    let namespace = entry::make_namespace(context, SYNC);
    entry::put_member(context, surface, "sync", namespace);
}

const SYNC: &[(&str, Provided)] = &[
    ("mutex_new", mutex_new),
    ("mutex_lock", mutex_lock),
    ("mutex_unlock", mutex_unlock),
    ("mutex_set", mutex_set),
    ("mutex_free", mutex_free),
];

/// A mutex cell: the value it guards, and whether it is currently held.
/// `locked` is not read by anything but `unlock` today — see the module doc
/// for why a second holder to refuse does not exist yet.
struct Mutex {
    value: i64,
    locked: bool,
}

thread_local! {
    static MUTEXES: RefCell<Vec<Option<Mutex>>> = RefCell::new(Vec::new());
}

/// The 0-based index a handle names, if any.
fn index_of(handle: u64) -> Option<usize> {
    let at = entry::number_of(handle)?;
    if !at.is_finite() || at < 1.0 {
        return None;
    }
    (at as usize).checked_sub(1)
}

/// `sync.mutex_new(initial)`.
extern "C" fn mutex_new(_e: u64, _this: u64, initial: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let value = entry::number_of(initial).unwrap_or(0.0) as i64;
    MUTEXES.with(|mutexes| {
        let mut mutexes = mutexes.borrow_mut();
        mutexes.push(Some(Mutex { value, locked: false }));
        entry::make_number(mutexes.len() as f64)
    })
}

/// `sync.mutex_lock(handle)` — answers the value the mutex guards. See the
/// module doc: this never blocks, because nothing here can ask twice at once.
extern "C" fn mutex_lock(_e: u64, _this: u64, handle: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    MUTEXES.with(|mutexes| {
        let mut mutexes = mutexes.borrow_mut();
        match index_of(handle).and_then(|at| mutexes.get_mut(at)) {
            Some(Some(mutex)) => {
                mutex.locked = true;
                entry::make_number(mutex.value as f64)
            }
            _ => entry::undefined_value(),
        }
    })
}

/// `sync.mutex_unlock(handle)`.
extern "C" fn mutex_unlock(_e: u64, _this: u64, handle: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    MUTEXES.with(|mutexes| {
        let mut mutexes = mutexes.borrow_mut();
        if let Some(Some(mutex)) = index_of(handle).and_then(|at| mutexes.get_mut(at)) {
            mutex.locked = false;
        }
    });
    entry::undefined_value()
}

/// `sync.mutex_set(handle, value)` — writes the guarded value. The caller is
/// expected to hold the lock, same as the old engine's surface; nothing here
/// checks, because refusing an unlocked write needs a second asker to protect
/// against and there is none yet.
extern "C" fn mutex_set(_e: u64, _this: u64, handle: u64, value: u64, _a2: u64, _a3: u64) -> u64 {
    let value = entry::number_of(value).unwrap_or(0.0) as i64;
    MUTEXES.with(|mutexes| {
        let mut mutexes = mutexes.borrow_mut();
        if let Some(Some(mutex)) = index_of(handle).and_then(|at| mutexes.get_mut(at)) {
            mutex.value = value;
        }
    });
    entry::undefined_value()
}

/// `sync.mutex_free(handle)` — safe regardless of lock state. See the module
/// doc for why: nothing holds a reference into the cell this frees.
extern "C" fn mutex_free(_e: u64, _this: u64, handle: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    MUTEXES.with(|mutexes| {
        let mut mutexes = mutexes.borrow_mut();
        if let Some(at) = index_of(handle) {
            if let Some(slot) = mutexes.get_mut(at) {
                *slot = None;
            }
        }
    });
    entry::undefined_value()
}
