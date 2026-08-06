//! `watch`/`watchFile`/`unwatchFile` — `FSWatcher`/`StatWatcher`, chained
//! onto the REAL `EventEmitter` prototype `crate::events` now builds (see its
//! module doc), over `notify` (OS change notification) and a polling
//! `std::thread` (stat-diff), per `Cargo.toml`.
//!
//! # WHEN a listener runs — read this before relying on `watch`
//!
//! A watcher's own thread — `notify`'s background OS-notification thread for
//! [`fs_watch`], this module's own polling thread for [`watch_file`] — is
//! NOT the JS thread, and this engine's context is thread-local
//! (`rts_core_rwk::entry::current`'s own doc: "aborts when there is none").
//! Calling a JS listener from that thread would run against no installed
//! context and abort the process on the first event, so neither thread ever
//! calls into JS. Instead each one only ever pushes a small native record —
//! never a JS value — into [`WATCHERS`], a plain `std::sync::Mutex`.
//!
//! Delivery happens on the JS thread, inside [`pump`], and [`pump`] is wired
//! into [`super::text`] and [`super::number`] — the two argument-reading
//! helpers nearly every native in this module calls first. So: **a queued
//! event's listener runs synchronously, just before the NEXT call this
//! module's own natives make into either helper** — in practice, at the
//! start of the next `fs`/`fs.promises` call the program issues (including
//! another `fs.watch`/`.close()`/etc. call), on whichever thread issues it.
//! A program that calls no further `fs` function after starting a watcher
//! **never observes its listener fire** — there is no timer, no event loop,
//! and no other point this engine hands control back to in a way this crate
//! can hook. That is a real, named limit, not the worst of the three options
//! this task asked to choose between (queue-and-deliver vs. refuse
//! entirely): a program that already talks to `fs` again (which is the
//! overwhelmingly common shape — `watch` a directory, then `readFileSync`
//! what changed) observes its listeners; one that starts a watcher and
//! merely waits does not, and is told so here rather than discovering a
//! watcher that silently never fires.
//!
//! # The `u64` a [`WatcherEntry`] holds
//!
//! [`WatcherEntry::instance`] is the `FSWatcher`/`StatWatcher` JS object,
//! kept so [`pump`] can call `.emit` on it later. This is the one place in
//! `rts-node-rwk` that holds a JS value across more than one native call (see
//! `fs/mod.rs`'s and `fs/dir.rs`'s doc for why every other table here keys by
//! a native id instead) — every entry point this crate's `entry` module
//! exports assumes a moving GC is not something a HOST needs to account for
//! (`rts_core_rwk::heap`'s handle table is the abstraction that would absorb
//! a move, and nothing here bypasses it: `instance` is read back out of the
//! SAME table by the SAME thread that put it there, never dereferenced as a
//! raw address), so this holds so long as that continues to be true. Named
//! rather than assumed, because it is the one assumption this module adds
//! that the rest of this crate does not need.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rts_core_rwk::entry::{self, Provided};

/// One observation, native data only — nothing here is a JS value, so
/// nothing here needs a context to build.
enum Queued {
    /// `FSWatcher`'s `'change'` event: `(eventType, filename)`.
    Fs { event_type: &'static str, filename: Option<String> },
    /// `StatWatcher`'s `'change'` event: `(curr, prev)`, built into `Stats`
    /// only once [`pump`] is ready to hand them to a listener.
    Stat { curr: std::fs::Metadata, prev: std::fs::Metadata },
}

struct WatcherEntry {
    /// See the module doc's last section.
    instance: u64,
    queue: VecDeque<Queued>,
    closed: bool,
    /// Keeps a `notify` watcher's OS subscription alive for as long as this
    /// entry exists; dropped (ending it) on `close()`.
    handle: Option<RecommendedWatcher>,
    /// Told to a polling thread to stop; absent for a `notify`-backed entry,
    /// which is stopped by dropping `handle` instead.
    stop: Option<Arc<AtomicBool>>,
}

static WATCHERS: Mutex<Option<HashMap<u64, WatcherEntry>>> = Mutex::new(None);
/// `watchFile`'s path → the `StatWatcher` ids registered against it, so
/// `unwatchFile` can find them. See [`unwatch_file`] for the one thing this
/// does not distinguish (a specific listener).
static PATH_WATCHERS: Mutex<Option<HashMap<String, Vec<u64>>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_watchers<T>(body: impl FnOnce(&mut HashMap<u64, WatcherEntry>) -> T) -> T {
    let mut guard = WATCHERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

fn with_path_watchers<T>(body: impl FnOnce(&mut HashMap<String, Vec<u64>>) -> T) -> T {
    let mut guard = PATH_WATCHERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// Delivers every queued event, oldest first per watcher, THEN drops the
/// [`WATCHERS`] lock before calling anything — a listener that itself calls
/// back into this module (another `fs` op) must not deadlock on a lock this
/// function is still holding.
pub(super) fn pump() {
    let due: Vec<(u64, u64, Vec<Queued>)> = with_watchers(|table| {
        table
            .iter_mut()
            .filter(|(_, entry)| !entry.closed && !entry.queue.is_empty())
            .map(|(&id, entry)| (id, entry.instance, entry.queue.drain(..).collect()))
            .collect()
    });
    if due.is_empty() {
        return;
    }
    let absent = entry::undefined_value();
    for (_id, instance, events) in due {
        let emit_fn = entry::with_runtime(|context| entry::get_member(context, instance, "emit"));
        for queued in events {
            match queued {
                Queued::Fs { event_type, filename } => {
                    let event_type = super::string(event_type);
                    let filename = match filename {
                        Some(name) => super::string(&name),
                        None => entry::null_value(),
                    };
                    let change = super::string("change");
                    entry::call(emit_fn, instance, change, event_type, filename, absent);
                }
                Queued::Stat { curr, prev } => {
                    let curr_stats = super::stats::build(&curr);
                    let prev_stats = super::stats::build(&prev);
                    let change = super::string("change");
                    entry::call(emit_fn, instance, change, curr_stats, prev_stats, absent);
                }
            }
        }
    }
}

/// Chains a fresh, empty-bodied prototype onto the real `EventEmitter` one —
/// `crate::events::namespace` always runs before any JS the program wrote
/// can call `fs.watch`/`watchFile` (see `lib.rs::install`), so by the time
/// this runs `"EventEmitter"` already names the SHARED, fully-built
/// prototype; the empty method list here is never installed anywhere new,
/// only read back.
fn chained_prototype(context: &mut entry::Context, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
    let prototype = entry::make_prototype(context, name, methods);
    entry::set_prototype(prototype, event_emitter);
    prototype
}

fn watch_id_of(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, super::string("__watchId"));
    entry::number_of(value).map(|value| value as u64)
}

// -------------------------------------------------------------- FSWatcher --

const FS_METHODS: &[(&str, Provided)] = &[("close", fs_close), ("ref", noop_self), ("unref", noop_self)];

/// (eventType, filename) out of one `notify::Event`. `'create'`/`'remove'`
/// read as Node's `'rename'`; everything else (content/metadata `'modify'`)
/// reads as `'change'` — matching Node's own coarse two-way split.
fn classify(event: &Event) -> (&'static str, Option<String>) {
    let event_type = match event.kind {
        EventKind::Create(_) | EventKind::Remove(_) => "rename",
        _ => "change",
    };
    let filename = event.paths.first().and_then(|path| path.file_name()).map(|name| name.to_string_lossy().into_owned());
    (event_type, filename)
}

fn start_fs_watch(id: u64, path: &str, recursive: bool) -> Option<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        let Ok(event) = result else { return };
        let (event_type, filename) = classify(&event);
        with_watchers(|table| {
            if let Some(entry) = table.get_mut(&id)
                && !entry.closed
            {
                entry.queue.push_back(Queued::Fs { event_type, filename });
            }
        });
    })
    .ok()?;
    // Linux (inotify) does not support a recursive subscription at all —
    // `notify` itself falls back to non-recursive there even when asked for
    // `RecursiveMode::Recursive`; this passes the caller's own request
    // through rather than pretending to detect the platform, matching the
    // "not our fallback to invent" call the reference doc's §7 leaves open.
    let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
    watcher.watch(Path::new(path), mode).ok()?;
    Some(watcher)
}

/// `fs.watch(filename, options?, listener?)`. `undefined` when the path
/// cannot be subscribed to (missing, no permission, platform refusal).
pub(super) extern "C" fn watch(_e: u64, _this: u64, filename: u64, options: u64, listener: u64, _a3: u64) -> u64 {
    let Some(path) = super::text(filename) else {
        return entry::undefined_value();
    };
    let recursive = super::option_flag(options, "recursive");
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let Some(handle) = start_fs_watch(id, &path, recursive) else {
        return entry::undefined_value();
    };
    let (instance, on_fn) = entry::with_runtime(|context| {
        let prototype = chained_prototype(context, "FSWatcher", FS_METHODS);
        let instance = entry::make_instance(context, prototype);
        let events = entry::make_object(context);
        entry::put_member(context, instance, "__events__", events);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__watchId", id_value);
        let on_fn = entry::get_member(context, instance, "on");
        (instance, on_fn)
    });
    with_watchers(|table| {
        table.insert(id, WatcherEntry { instance, queue: VecDeque::new(), closed: false, handle: Some(handle), stop: None });
    });
    let absent = entry::undefined_value();
    if listener != absent {
        let change = super::string("change");
        entry::call(on_fn, instance, change, listener, absent, absent);
    }
    instance
}

extern "C" fn fs_close(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(id) = watch_id_of(this) {
        with_watchers(|table| {
            if let Some(entry) = table.get_mut(&id) {
                entry.closed = true;
                entry.handle = None;
                entry.queue.clear();
            }
        });
    }
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    let close_event = super::string("close");
    entry::call(emit_fn, this, close_event, absent, absent, absent);
    entry::undefined_value()
}

extern "C" fn noop_self(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    this
}

// ------------------------------------------------------------- StatWatcher --

const STAT_METHODS: &[(&str, Provided)] = &[("ref", noop_self), ("unref", noop_self)];

fn metadata_changed(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    a.len() != b.len() || a.modified().ok() != b.modified().ok()
}

fn spawn_poll(id: u64, path: String, interval: Duration, stop: Arc<AtomicBool>, mut previous: Option<std::fs::Metadata>) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            if stop.load(Ordering::SeqCst) {
                return;
            }
            let current = std::fs::metadata(&path).ok();
            if let (Some(curr), Some(prev)) = (&current, &previous)
                && metadata_changed(curr, prev)
            {
                let curr = curr.clone();
                let prev = prev.clone();
                with_watchers(|table| {
                    if let Some(entry) = table.get_mut(&id)
                        && !entry.closed
                    {
                        entry.queue.push_back(Queued::Stat { curr, prev });
                    }
                });
            }
            previous = current;
        }
    });
}

/// `fs.watchFile(filename, options?, listener)` — see the module doc; no
/// sync/promise form exists in real Node either. `options.interval`
/// defaults to `5007`ms, matching the reference doc. `options.persistent` is
/// read as nothing here (this engine keeps no process alive on a
/// background thread's account) — named rather than silently honored.
pub(super) extern "C" fn watch_file(_e: u64, _this: u64, filename: u64, options: u64, listener: u64, _a3: u64) -> u64 {
    let Some(path) = super::text(filename) else {
        return entry::undefined_value();
    };
    let interval_ms = {
        let absent = entry::undefined_value();
        if options == absent {
            5007.0
        } else {
            let value = entry::with_runtime(|context| entry::get_member(context, options, "interval"));
            entry::number_of(value).unwrap_or(5007.0)
        }
    };
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let stop = Arc::new(AtomicBool::new(false));
    let initial = std::fs::metadata(&path).ok();
    spawn_poll(id, path.clone(), Duration::from_millis(interval_ms.max(1.0) as u64), stop.clone(), initial);
    let (instance, on_fn) = entry::with_runtime(|context| {
        let prototype = chained_prototype(context, "StatWatcher", STAT_METHODS);
        let instance = entry::make_instance(context, prototype);
        let events = entry::make_object(context);
        entry::put_member(context, instance, "__events__", events);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__watchId", id_value);
        let on_fn = entry::get_member(context, instance, "on");
        (instance, on_fn)
    });
    with_watchers(|table| {
        table.insert(id, WatcherEntry { instance, queue: VecDeque::new(), closed: false, handle: None, stop: Some(stop) });
    });
    with_path_watchers(|table| table.entry(path).or_default().push(id));
    let absent = entry::undefined_value();
    if listener != absent {
        let change = super::string("change");
        entry::call(on_fn, instance, change, listener, absent, absent);
    }
    instance
}

/// `fs.unwatchFile(filename, listener?)`. Stops every `StatWatcher`
/// registered against `filename` regardless of `listener` — real Node stops
/// only the ONE matching registration when `listener` is given; matching a
/// specific listener would mean walking `__events__` on every watcher for
/// this path and is not implemented, named here rather than silently
/// ignored (the effect a caller sees, "stop watching too much rather than
/// too little", is the safer direction to be wrong in for a watch API).
pub(super) extern "C" fn unwatch_file(_e: u64, _this: u64, filename: u64, _listener: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(path) = super::text(filename) {
        let ids = with_path_watchers(|table| table.remove(&path).unwrap_or_default());
        for id in ids {
            with_watchers(|table| {
                if let Some(entry) = table.get_mut(&id) {
                    entry.closed = true;
                    entry.queue.clear();
                    if let Some(stop) = &entry.stop {
                        stop.store(true, Ordering::SeqCst);
                    }
                }
            });
        }
    }
    entry::undefined_value()
}
