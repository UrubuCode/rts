//! `Dir`/`opendirSync` — a cursor over one directory's entries, over one
//! shared prototype (see [`super::dirent`] for why a prototype and not a
//! closure per instance).
//!
//! # A snapshot, not a live cursor
//!
//! `std::fs::ReadDir` is a real Rust iterator, but nothing in this crate's
//! value API can hold a Rust iterator *inside* a JS object — an instance's
//! own data lives in the shape-and-property system, addressed by name, and a
//! `std::fs::ReadDir` is not a value that system knows how to hold. So
//! `opendirSync` reads the WHOLE directory up front into this module's own
//! table ([`TABLE`], keyed the same way [`super::fd::TABLE`] keys open
//! files), and `read()`/`readSync()` walk that snapshot rather than the live
//! directory.
//!
//! The divergence from Node this trades for: Node's `Dir` is a live OS
//! cursor, so an entry created after `opendirSync` returns but before a
//! matching `read()` call can still appear, and one removed can vanish
//! mid-walk. Here, `opendirSync` fixes the list once. A caller iterating a
//! directory that is not concurrently changing — the ordinary case — sees no
//! difference.
//!
//! # `read()` is synchronous here
//!
//! Real Node's `read()` (no `Sync` suffix) is asynchronous — it returns a
//! `Promise` and also accepts a callback. This module has no background I/O
//! anywhere (see the parent module doc), so `read()` answers synchronously,
//! same as `readSync()`; the two names are one function.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core::entry::{self, Provided};

struct Cursor {
    path: String,
    entries: VecDeque<(String, u32)>,
    closed: bool,
}

static TABLE: Mutex<Option<HashMap<u64, Cursor>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, Cursor>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[
    ("read", read),
    ("readSync", read),
    ("close", close),
    ("closeSync", close),
];

/// `fs.opendirSync(path)`. `undefined` on failure.
pub(super) extern "C" fn opendir_sync(_e: u64, _this: u64, path: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(path) = super::text(path) else {
        return entry::undefined_value();
    };
    let Ok(entries) = std::fs::read_dir(&path) else {
        return entry::undefined_value();
    };
    let collected: VecDeque<(String, u32)> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let bits = entry.file_type().map(super::dirent::type_bits).unwrap_or(0);
            (name, bits)
        })
        .collect();
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| {
        table.insert(id, Cursor { path: path.clone(), entries: collected, closed: false });
    });
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Dir", METHODS);
        let instance = entry::make_instance(context, prototype);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__dirId", id_value);
        let path_value = entry::make_string(context, &path);
        entry::put_member(context, instance, "path", path_value);
        instance
    })
}

fn id_of(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, super::string("__dirId"));
    entry::number_of(value).map(|value| value as u64)
}

/// `dir.read()`/`dir.readSync()` — the next [`super::dirent`], or `null` once
/// the snapshot is exhausted or the cursor is closed.
extern "C" fn read(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::null_value();
    };
    let next = with_table(|table| {
        table.get_mut(&id).and_then(|cursor| match cursor.closed {
            true => None,
            false => cursor.entries.pop_front().map(|(name, bits)| (name, bits, cursor.path.clone())),
        })
    });
    match next {
        Some((name, bits, path)) => entry::with_runtime(|context| super::dirent::build(context, &name, &path, bits)),
        None => entry::null_value(),
    }
}

/// `dir.close()`/`dir.closeSync()`.
extern "C" fn close(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(id) = id_of(this) {
        with_table(|table| {
            if let Some(cursor) = table.get_mut(&id) {
                cursor.closed = true;
                cursor.entries.clear();
            }
        });
    }
    entry::undefined_value()
}
