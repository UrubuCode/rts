//! `node:sqlite` — `DatabaseSync`/`StatementSync` over `turso_core`, a
//! pure-Rust, SQLite-file-compatible engine (`docs/reference/node/sqlite.md`
//! §5.1 names it; `crates.md` §4.12 pins the feature set that keeps its build
//! pure Rust — `pure-rust-crypto` forces the `softaes` backend so stock
//! defaults never pull `aegis`, which `cc`-compiles).
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search (§1, the machine) found
//! nothing to call: `rts-cranelift` has no SQL engine and no file-backed value
//! store, its table is entirely value encodings and ABI. Inside this crate,
//! `crate::fs::dir` is the answer to "state that is not a JS value" — the
//! connection and statement tables here are that same recipe, a
//! `Mutex`-backed table keyed by a number the instance carries, not a second
//! derivation of it. Every value crossing the boundary goes through
//! `rts_core::entry::modules` — the only value API this crate reaches —
//! and [`value`] states, once, exactly what each SQLite storage class becomes
//! and what does not cross.
//!
//! # Every extern here is driven synchronously, in-thread
//!
//! `turso_core`'s own async surface (`IOResult`, `Completion`) is real, but
//! `Database::open_file` and `Statement::run_ignore_rows`/`run_collect_rows`
//! already drive it to completion internally — the same "advance IO in a
//! blocking loop" turso_core's own `pragma_query` is written over. Nothing
//! in this module holds a `Completion` across a call boundary, so there is no
//! second event loop to reconcile with this engine's.
//!
//! # Not implemented, by name
//!
//! **`Session`/`createSession`/`changeset`/`patchset`/`applyChangeset`** — the
//! SQLite session extension is a separate compiled feature `turso_core` does
//! not expose through its public API at this version; nothing here fakes a
//! changeset format.
//! **`SQLTagStore`/`createTagStore`** — an LRU cache of prepared statements
//! keyed by a tagged template; every other module in this crate builds that
//! kind of convenience layer in a `.ts` shim, which is not this module's to
//! write (`Cargo.toml`, `lib.rs`, and every prelude `.ts` file are owned
//! elsewhere for this change).
//! **`aggregate`/`function`/`setAuthorizer`** — each needs a JS `Function`
//! invoked synchronously and re-entrantly from inside a `turso_core` callback
//! mid-`step()`; `turso_core`'s public API has no scalar/aggregate function
//! or authorizer registration hook to attach one to (that surface belongs to
//! `libsqlite3-sys`-style bindings this crate deliberately does not depend
//! on — see `crates.md` §4.12's rejection of `rusqlite`).
//! **`enableDefensive`/`enableLoadExtension`/`loadExtension`** — no
//! `sqlite3_db_config`/`sqlite3_load_extension` equivalent in `turso_core`'s
//! public API; loading a native extension is specifically the capability a
//! pure-Rust engine does not have.
//! **`limits`, `[Symbol.dispose]`** — `limits` has no `sqlite3_limit()`
//! equivalent to read live; `[Symbol.dispose]` needs a symbol-keyed member,
//! which `entry::make_prototype` cannot install (string-named members only,
//! the same gap [`statement`] states for `iterate()`'s `Symbol.iterator`).
//! **`backup()`** — `turso_core`'s public API has no
//! `sqlite3_backup_init`/`_step`/`_finish` equivalent; refused rather than
//! reimplemented as a `VACUUM INTO`-style file copy, which is a different
//! operation with different online-consistency guarantees than Node's
//! `backup()` promises.
//! **`open()`/`[re]open after close`** — `DatabaseSync`'s own `open()` method
//! (only meaningful after `{ open: false }`) is not built; construct with
//! `open` defaulting to `true` (see [`database::construct`]) covers the
//! common case, and a closed connection stays closed.
//! **Every `DatabaseSyncOptions` field but `open`/`readOnly`** —
//! `enableForeignKeyConstraints`, `enableDoubleQuotedStringLiterals`,
//! `timeout`, `defensive`, `allowExtension`, `allowBareNamedParameters`,
//! `allowUnknownNamedParameters` are accepted (silently ignored) by
//! `DatabaseSyncOptions`-shaped input reaching no code, rather than being
//! individually parsed and dropped — each would need a `turso_core` pragma or
//! connection setter this module has not wired up yet.
//!
//! See [`statement`]'s doc for what does not cross a parameter/row boundary,
//! and [`value`]'s for the storage-class conversion table.

mod database;
mod statement;
mod value;

use rts_core::entry::{self, Context};

/// The `node:sqlite` namespace: `DatabaseSync` and `constants`.
pub fn namespace(context: &mut Context) -> u64 {
    let namespace = entry::make_namespace(context, &[]);
    let ctor = entry::make_callable(context, database::construct);
    let prototype = database::prototype(context);
    entry::put_member(context, ctor, "prototype", prototype);
    // The `prototype.constructor` back-link `make_callable` never writes —
    // see `crate::stream::class_ctor`'s doc for the mechanism (why every
    // hand-built `node:` class answered `"Object"` to `.constructor.name`
    // without this call) and why `new WASI()` returning `undefined` was the
    // SAME gap's other half (`crate::wasi::namespace`'s doc).
    entry::declare_host_class(context, ctor, prototype, "DatabaseSync", 1);
    entry::put_member(context, namespace, "DatabaseSync", ctor);
    let constants = constants(context);
    entry::put_member(context, namespace, "constants", constants);
    namespace
}

/// `sqlite.constants` — the `SQLITE_*` open flags and the conflict-resolution
/// / authorizer-outcome codes `docs/reference/node/sqlite.md` §2.3 lists.
/// Plain numbers: nothing here is wired to a feature that consumes them
/// (session/authorizer are both refused, see the module doc), so they are
/// exposed for a caller comparing against them, matching Node's constants
/// even where this module cannot yet act on the comparison's result.
fn constants(context: &mut Context) -> u64 {
    let object = entry::make_object(context);
    let entries: &[(&str, f64)] = &[
        ("SQLITE_OPEN_READONLY", 0x0000_0001 as f64),
        ("SQLITE_OPEN_READWRITE", 0x0000_0002 as f64),
        ("SQLITE_OPEN_CREATE", 0x0000_0004 as f64),
        ("SQLITE_OPEN_URI", 0x0000_0040 as f64),
        ("SQLITE_OPEN_MEMORY", 0x0000_0080 as f64),
        ("SQLITE_CHANGESET_OMIT", 0.0),
        ("SQLITE_CHANGESET_REPLACE", 1.0),
        ("SQLITE_CHANGESET_ABORT", 2.0),
        ("SQLITE_CHANGESET_DATA", 1.0),
        ("SQLITE_CHANGESET_NOTFOUND", 2.0),
        ("SQLITE_CHANGESET_CONFLICT", 3.0),
        ("SQLITE_CHANGESET_CONSTRAINT", 4.0),
        ("SQLITE_CHANGESET_FOREIGN_KEY", 5.0),
        ("SQLITE_OK", 0.0),
        ("SQLITE_DENY", 1.0),
        ("SQLITE_IGNORE", 2.0),
    ];
    for (name, value) in entries {
        let number = entry::make_number(*value);
        entry::put_member(context, object, name, number);
    }
    object
}
