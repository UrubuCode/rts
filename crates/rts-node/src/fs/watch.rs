//! node:fs — `watch(path, listener)` → an `FSWatcher`. Real OS filesystem
//! notifications via the `notify` crate (inotify / kqueue / ReadDirectoryChangesW).
//! The watcher's notification thread pushes plain event data into the engine's
//! cross-crate [`rts_engine::watch_queue`]; the event loop drains it on the JS
//! thread and invokes the listener `(eventType, filename)`. The loop stays alive
//! while any watcher is open (Node semantics), so a script that `watch`es keeps
//! running until `watcher.close()`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::poly_handle_normalize;
use rts_engine::watch_queue;

use super::words::read;

use rts_engine::gc_surface::{__RTS_FN_NS_GC_PIN_HANDLE, __RTS_FN_NS_GC_UNPIN_HANDLE};

/// Live watchers, keyed by id. Keeping the `RecommendedWatcher` here keeps the OS
/// watch active; dropping it (on `close`) stops the notifications.
fn watchers() -> &'static Mutex<HashMap<u64, RecommendedWatcher>> {
    static W: OnceLock<Mutex<HashMap<u64, RecommendedWatcher>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn str_handle(s: &str) -> i64 {
    alloc_entry(Entry::String(s.as_bytes().to_vec())) as i64
}

/// `fs.watch(path, listener)` → `FSWatcher`. `listener(eventType, filename)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WATCH(p: *const u8, l: i64, listener: u64) -> u64 {
    let path = read(p, l);
    // The listener is a function VALUE (boxed); resolve its real handle and PIN it
    // so the GC keeps it alive for the watcher's lifetime.
    let cb = poly_handle_normalize(listener).unwrap_or(listener);
    unsafe { __RTS_FN_NS_GC_PIN_HANDLE(cb) };

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let watched = path.clone();
    let handler = move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            // rename for create/remove/name-change, else change.
            let kind: u8 = match event.kind {
                EventKind::Create(_) | EventKind::Remove(_) => 0,
                EventKind::Modify(notify::event::ModifyKind::Name(_)) => 0,
                _ => 1,
            };
            for pb in &event.paths {
                let filename = pb
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| watched.clone());
                watch_queue::push(cb, kind, filename);
            }
        }
    };

    match notify::recommended_watcher(handler) {
        Ok(mut w) => {
            // A file target watches its parent dir (notify watches paths); a dir
            // target watches itself. NonRecursive matches Node's default.
            let target = Path::new(&path);
            let watch_path = if target.is_dir() { target } else { target.parent().unwrap_or(target) };
            if w.watch(watch_path, RecursiveMode::NonRecursive).is_ok() {
                watchers().lock().unwrap().insert(id, w);
                watch_queue::inc_active();
            } else {
                unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
            }
        }
        Err(_) => unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) },
    }

    let mut m: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    m.insert("__rts_class".to_string(), str_handle("FSWatcher"));
    m.insert("__id".to_string(), id as i64);
    m.insert("__cb".to_string(), cb as i64);
    alloc_entry(Entry::Map(Box::new(m)))
}

fn field(this: u64, key: &str) -> i64 {
    with_entry(this, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0),
        _ => 0,
    })
}

/// `fswatcher.close()` — stop the OS watch, release the loop's keep-alive, and
/// unpin the listener. Idempotent (a second close is a no-op).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_FS_WATCHER_CLOSE(this: u64) {
    let id = field(this, "__id") as u64;
    if id == 0 {
        return;
    }
    let removed = watchers().lock().unwrap().remove(&id).is_some();
    if removed {
        watch_queue::dec_active();
        let cb = field(this, "__cb") as u64;
        if cb != 0 {
            unsafe { __RTS_FN_NS_GC_UNPIN_HANDLE(cb) };
        }
        // Mark closed so a repeat close is a no-op.
        rts_engine::heap::handles::with_entry_mut(this, |e| {
            if let Some(Entry::Map(m)) = e {
                m.insert("__id".to_string(), 0);
            }
        });
    }
}
