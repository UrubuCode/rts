//! node:fs — `watchFile(path, listener)` / `unwatchFile(path)`. Polling watcher:
//! a background thread stats `path` on an interval and, when its mtime changes,
//! pushes a `kind=2` event into [`rts_engine::watch_queue`]. The event loop then
//! calls [`__RTS_FN_NODE_FS_WATCHFILE_FIRE`] on the JS thread, which builds the
//! `(curr, prev)` `Stats` arguments Node's listener expects and invokes it.
//! `unwatchFile(path)` stops the thread. Real `std::fs` stat polling, no stubs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, UNIX_EPOCH};

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::poly::poly_handle_normalize;
use rts_engine::heap::shapes::handle_word_auto;
use rts_engine::watch_queue;

use super::stats;
use super::words::read;

unsafe extern "C" {
    fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
    fn __RTS_FN_NS_GC_PIN_HANDLE(handle: u64);
    fn __RTS_FN_NS_GC_UNPIN_HANDLE(handle: u64);
}

/// Active polling watchers: `path` → its running flag (cleared by `unwatchFile`).
fn watchers() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static W: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The previous `Stats` handle per watched path (pinned across fires so the GC
/// keeps it alive to pass as the listener's `prev`).
fn prev_stats() -> &'static Mutex<HashMap<String, u64>> {
    static P: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A change signature `(mtime_nanos, size)` — comparing BOTH catches a change even
/// when two writes land in the same clock tick (the size differs) or a rewrite
/// keeps the same size (the mtime differs). `(-1, -1)` when the file is absent.
fn change_sig(path: &str) -> (i128, u64) {
    match std::fs::metadata(path) {
        Ok(m) => {
            let ns = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as i128)
                .unwrap_or(-1);
            (ns, m.len())
        }
        Err(_) => (-1, u64::MAX),
    }
}

/// `fs.watchFile(path, listener)` — poll `path` and invoke `listener(curr, prev)`
/// on each change.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WATCHFILE0(p: *const u8, l: i64, listener: u64) {
    watch_file(read(p, l), listener, 500);
}

/// `fs.watchFile(path, intervalMs, listener)` — the interval-configurable form.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WATCHFILE_OPTS(p: *const u8, l: i64, interval: i64, listener: u64) {
    let ms = if interval > 0 { interval as u64 } else { 500 };
    watch_file(read(p, l), listener, ms);
}

/// Start polling `path` every `interval` ms (default 500 — faster than Node's
/// 5007 so a change is seen within the event loop's idle window), invoking
/// `listener(curr, prev)` on each mtime change.
fn watch_file(path: String, listener: u64, interval: u64) {
    let cb = poly_handle_normalize(listener).unwrap_or(listener);
    unsafe { __RTS_FN_NS_GC_PIN_HANDLE(cb) };

    let flag = Arc::new(AtomicBool::new(true));
    // Replace any existing watcher on this path (Node allows re-watch).
    if let Some(old) = watchers().lock().unwrap().insert(path.clone(), flag.clone()) {
        old.store(false, Ordering::Relaxed);
    } else {
        watch_queue::inc_active();
    }

    // Seed `prev` with the file's CURRENT Stats so the first change delivers the
    // real prior state as `prev` (not a copy of `curr`). Built here on the JS
    // thread (GC-safe) and pinned for the watcher's lifetime.
    if prev_stats().lock().unwrap().get(&path).is_none() {
        if let Ok(md) = std::fs::metadata(&path) {
            let init = handle_word_auto(stats::build(&md));
            unsafe { __RTS_FN_NS_GC_PIN_HANDLE(init) };
            prev_stats().lock().unwrap().insert(path.clone(), init);
        }
    }

    // Capture the baseline mtime NOW (synchronously, at the watchFile call) — not
    // inside the spawned thread, which could start after the caller's next write
    // and miss the very change it is meant to detect.
    let mut last = change_sig(&path);
    let thread_path = path.clone();
    std::thread::spawn(move || {
        while flag.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(interval));
            if !flag.load(Ordering::Relaxed) {
                break;
            }
            let now = change_sig(&thread_path);
            if now != last {
                last = now;
                watch_queue::push(cb, 2, thread_path.clone());
            }
        }
    });
}

/// `fs.unwatchFile(path)` — stop polling `path`, release the loop keep-alive.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_UNWATCHFILE(p: *const u8, l: i64) {
    let path = read(p, l);
    if let Some(flag) = watchers().lock().unwrap().remove(&path) {
        flag.store(false, Ordering::Relaxed);
        watch_queue::dec_active();
    }
    // Release the pinned prev Stats for this path.
    if let Some(prev) = prev_stats().lock().unwrap().remove(&path) {
        unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(prev) };
    }
}

/// Called by the event loop (JS thread) for a `watchFile` change: build the
/// current `Stats`, pair it with the stored previous `Stats`, and invoke
/// `listener(curr, prev)`. `curr` becomes the next `prev`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WATCHFILE_FIRE(listener: u64, path_ptr: *const u8, path_len: i64) {
    let path = read(path_ptr, path_len);
    let Ok(md) = std::fs::metadata(&path) else {
        return; // file vanished — skip this tick (a delete/recreate settles next)
    };
    let curr = handle_word_auto(stats::build(&md));
    unsafe { __RTS_FN_NS_GC_PIN_HANDLE(curr) };

    let prev = {
        let mut map = prev_stats().lock().unwrap();
        map.insert(path.clone(), curr).unwrap_or(curr)
    };
    // listener(curr, prev) — via the handle-based invoke (the listener is a
    // Function handle; `INVOKE_AUTO` takes a two-element args array).
    let args = alloc_entry(Entry::Vec(Box::new(vec![curr as i64, prev as i64])));
    unsafe {
        __RTS_FN_RT_INVOKE_AUTO(listener as i64, 0, args);
        if prev != curr {
            __RTS_FN_NS_GC_UNPIN_HANDLE(prev);
        }
    }
}
