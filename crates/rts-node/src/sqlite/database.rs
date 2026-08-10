//! `DatabaseSync` — one open (or not-yet-opened) `turso_core` connection.
//!
//! # State that is not a JS value
//!
//! An `Arc<turso_core::Connection>` cannot live inside this engine's object
//! system — the same gap `crates/rts-node/src/fs/dir.rs` documents for a
//! `std::fs::ReadDir` — so it lives in [`TABLE`], keyed by a number the
//! instance carries under a hidden property, exactly that module's pattern.
//!
//! # `isOpen`/`isTransaction` are data properties, not accessors
//!
//! Node's are getters. `entry::modules` — the only value API this crate is
//! allowed to reach — has no accessor installer (`entry::define_getter`
//! exists but is not part of that surface), so this module refreshes two
//! plain data properties on every call that can change them (`open`, `close`,
//! `exec`) instead. A program that reads `db.isOpen` right after a mutation
//! this module performed sees the current answer; the divergence from Node is
//! only that it is a value looked up rather than a function run at read time,
//! which is not observable from JS.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rts_core::entry::{self, Context, Provided};
use turso_core::{Connection, Database, MemoryIO, OpenFlags, PlatformIO, IO};

use super::statement;

pub(super) struct Conn {
    pub(super) conn: Arc<Connection>,
    path: String,
}

static TABLE: Mutex<Option<HashMap<u64, Conn>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, Conn>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[
    ("close", close),
    ("exec", exec),
    ("prepare", prepare),
    ("location", location),
];

/// The `DatabaseSync` prototype, made once and named — the same
/// [`entry::make_prototype`] pattern `fs::dir::opendir_sync` uses.
pub(super) fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "DatabaseSync", METHODS)
}

fn id_of(this: u64) -> Option<u64> {
    let value = entry::with_runtime(|context| entry::get_member(context, this, "__dbId"));
    entry::number_of(value).map(|value| value as u64)
}

fn option_value(context: &mut Context, options: u64, name: &str) -> u64 {
    let absent = entry::undefined_in(context);
    if options == absent { absent } else { entry::get_member(context, options, name) }
}

/// A boolean option's RAW value, undecoded.
///
/// # Why this does not call `to_boolean` itself
///
/// `entry::to_boolean` is ambient — it calls `with_current` itself — so
/// calling it from inside [`entry::with_runtime`]'s closure would be the
/// nested borrow `docs/reference/node/STATUS.md` names as the abort every
/// module here pays for. Every caller here decodes the raw value with
/// `entry::to_boolean` AFTER the borrow that fetched it ends, the same
/// two-step `dgram::construct` documents for `reuseAddr`.
fn option_raw(context: &mut Context, options: u64, name: &str) -> u64 {
    option_value(context, options, name)
}

fn set_bool(context: &mut Context, instance: u64, name: &str, value: bool) {
    entry::put_member(context, instance, name, entry::boolean_value(value));
}

/// Refreshes `isOpen`/`isTransaction` from the live connection — see the
/// module doc for why these are data properties.
fn refresh_flags(context: &mut Context, instance: u64, conn: &Connection) {
    set_bool(context, instance, "isOpen", !conn.is_closed());
    set_bool(context, instance, "isTransaction", !conn.get_auto_commit());
}

/// `new DatabaseSync(path, options?)`.
///
/// # Divergence: always constructs, never throws
///
/// A malformed `path` or a failed open is Node's `TypeError`/`ERR_SQLITE_ERROR`.
/// This crate has no way to raise a catchable exception from a native —
/// [`entry::throw`] ends the PROGRAM, it is not `throw` — so a failed open
/// answers an instance whose `isOpen` is `false` rather than refusing to
/// construct one at all. A caller checking `isOpen` after construction (which
/// `open: false` already requires them to do, per Node's own contract for
/// that option) observes the failure; nothing pretends the connection opened.
pub(super) extern "C" fn construct(_e: u64, _this: u64, path: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let (path_text, open_raw, read_only_raw) = entry::with_runtime(|context| {
        let path_text = entry::text_in(context, path).unwrap_or_else(|| ":memory:".to_owned());
        let open_raw = option_raw(context, options, "open");
        let read_only_raw = option_raw(context, options, "readOnly");
        (path_text, open_raw, read_only_raw)
    });
    // Decoded OUTSIDE the borrow above — see `option_raw`'s doc.
    let open_now = if open_raw == entry::undefined_value() { true } else { entry::to_boolean(open_raw) };
    let read_only = entry::to_boolean(read_only_raw);

    let mut conn: Option<Arc<Connection>> = None;
    if open_now {
        conn = open_connection(&path_text, read_only);
    }

    entry::with_runtime(|context| {
        let proto = prototype(context);
        let instance = entry::make_instance(context, proto);
        match conn {
            Some(conn) => {
                let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
                refresh_flags(context, instance, &conn);
                with_table(|table| {
                    table.insert(id, Conn { conn, path: path_text.clone() });
                });
                let id_value = entry::make_number(id as f64);
                entry::put_member(context, instance, "__dbId", id_value);
            }
            None => {
                set_bool(context, instance, "isOpen", false);
                set_bool(context, instance, "isTransaction", false);
            }
        }
        instance
    })
}

/// Opens a connection over the real filesystem, or `MemoryIO` for
/// `:memory:` / an empty path — matching `better-sqlite3`/Node's own
/// convention that both name a private, temporary, in-memory database.
///
/// `None` on any failure — `open_file`/`connect` returning `Err` covers a
/// missing directory, a corrupt file, or (for `readOnly`) a file that does
/// not exist yet, none of which this module can turn into a JS exception.
fn open_connection(path: &str, read_only: bool) -> Option<Arc<Connection>> {
    if path == ":memory:" || path.is_empty() {
        let io: Arc<dyn IO> = Arc::new(MemoryIO::new());
        let db = Database::open_file(io, ":memory:").ok()?;
        return db.connect().ok();
    }
    let io: Arc<dyn IO> = Arc::new(PlatformIO::new().ok()?);
    let flags = if read_only { OpenFlags::ReadOnly } else { OpenFlags::default() };
    let db = Database::open_file_with_flags(io, path, flags, turso_core::DatabaseOpts::new(), None).ok()?;
    db.connect().ok()
}

/// `db.close()`.
extern "C" fn close(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let conn = with_table(|table| table.get(&id).map(|entry| entry.conn.clone()));
    if let Some(conn) = conn {
        let _ = conn.close();
        entry::with_runtime(|context| refresh_flags(context, this, &conn));
    }
    entry::undefined_value()
}

/// `db.exec(sql)` — runs every statement in `sql`, discarding any rows. No
/// bound parameters, matching `sqlite3_exec()` and Node's own contract for
/// this method (`prepare()` is what reads rows back).
extern "C" fn exec(_e: u64, this: u64, sql: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let Some(sql_text) = entry::text_of(sql) else {
        return entry::undefined_value();
    };
    let conn = with_table(|table| table.get(&id).map(|entry| entry.conn.clone()));
    let Some(conn) = conn else {
        return entry::undefined_value();
    };
    // Errors are `undefined`, the same convention as everywhere else in this
    // crate (see `docs/reference/node/STATUS.md`'s `fs` cross-reference) —
    // there is no catchable exception this native could raise instead.
    let _ = conn.prepare_execute_batch(&sql_text);
    entry::with_runtime(|context| refresh_flags(context, this, &conn));
    entry::undefined_value()
}

/// `db.prepare(sql)`.
extern "C" fn prepare(_e: u64, this: u64, sql: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let Some(sql_text) = entry::text_of(sql) else {
        return entry::undefined_value();
    };
    let conn = with_table(|table| table.get(&id).map(|entry| entry.conn.clone()));
    let Some(conn) = conn else {
        return entry::undefined_value();
    };
    let Ok(stmt) = conn.prepare(&sql_text) else {
        return entry::undefined_value();
    };
    statement::make(id, stmt, sql_text)
}

/// `db.location(dbName?)` — the absolute path, or `null` for an in-memory
/// database. `dbName` is accepted but only `'main'` is answered: this module
/// tracks one connection to one file, never `ATTACH`ed databases.
extern "C" fn location(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::null_value();
    };
    let path = with_table(|table| table.get(&id).map(|entry| entry.path.clone()));
    match path {
        Some(path) if path != ":memory:" && !path.is_empty() => {
            entry::with_runtime(|context| rts_core::entry::make_string(context, &path))
        }
        _ => entry::null_value(),
    }
}
