//! `StatementSync` — one compiled `turso_core::Statement`, over the same
//! [`super::database::TABLE`] recipe keyed on its own [`TABLE`].
//!
//! # Binding is capped at four positional parameters
//!
//! `stmt.run(...params)` is variadic in Node. A native here has five value
//! slots total and the receiver takes one of them — the same limit
//! `crate::console::log` already accepts for its own variadic surface — so
//! `run`/`get`/`all` take up to **four** positional parameters directly as
//! `a0..a3`, not an array. A fifth is silently unreachable rather than
//! truncated data: this module treats a JS `undefined` in slot N as "no
//! parameter N" (the same sentinel a caller binding fewer than four params
//! relies on), so the only observable gap is a statement with five or more
//! `?` placeholders, named below.
//!
//! **Not implemented, by name**: named parameters (`:id`, `@name`, `$x`) —
//! this module binds positionally only, `1`-indexed in call order, which
//! answers a SQL text using bare `?` placeholders and nothing that inspects a
//! parameter's own name; a fifth-or-later positional parameter, for the
//! reason above; `iterate()` — Node's is a lazy `IterableIterator`, and
//! `entry::make_prototype` installs members by `&str` name only, with no way
//! to install a `Symbol.iterator`-keyed method, so a returned object could
//! not satisfy `for...of` even with the cursor state `all`'s own drain
//! already threads through this table — refused rather than shipped as an
//! object that merely LOOKS like an iterator; `setReturnArrays` — accepted
//! and stored, never consulted: every row answers a plain object, keyed by
//! [`turso_core::Statement::get_column_name`]; `expandedSQL` — there is no
//! `sqlite3_expanded_sql` equivalent in `turso_core`'s public surface to wrap.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rts_core::entry::{self, Context, Provided};
use turso_core::{Statement, StepResult};

use super::value;

struct Stmt {
    stmt: Statement,
    conn_id: u64,
    source_sql: String,
}

static TABLE: Mutex<Option<HashMap<u64, Stmt>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, Stmt>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

const METHODS: &[(&str, Provided)] = &[
    ("run", run),
    ("get", get),
    ("all", all),
    ("columns", columns),
];

fn prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "StatementSync", METHODS)
}

/// Builds the `StatementSync` instance `database::prepare` hands back.
pub(super) fn make(conn_id: u64, stmt: Statement, source_sql: String) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| {
        table.insert(id, Stmt { stmt, conn_id, source_sql: source_sql.clone() });
    });
    entry::with_runtime(|context| {
        let proto = prototype(context);
        let instance = entry::make_instance(context, proto);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__stmtId", id_value);
        let sql_value = entry::make_string(context, &source_sql);
        entry::put_member(context, instance, "sourceSQL", sql_value);
        instance
    })
}

fn id_of(this: u64) -> Option<u64> {
    let value = entry::with_runtime(|context| entry::get_member(context, this, "__stmtId"));
    entry::number_of(value).map(|value| value as u64)
}

/// Binds up to four positional parameters, `1`-indexed. A JS `undefined` in a
/// slot binds nothing — see the module doc for why that is the "not
/// provided" sentinel rather than an attempt to bind `NULL` there.
fn bind_positional(context: &Context, stmt: &mut Statement, args: [u64; 4]) {
    let undefined = entry::undefined_in(context);
    for (offset, arg) in args.into_iter().enumerate() {
        if arg == undefined {
            continue;
        }
        let Some(index) = std::num::NonZero::new(offset + 1) else {
            continue;
        };
        stmt.bind_at(index, value::from_js(context, arg));
    }
}

/// `stmt.run(...)` — one step cycle for a non-row-producing statement.
/// `{ changes, lastInsertRowid }`, both as `number` (see [`super::value`] for
/// why `lastInsertRowid` past 2^53 has already lost precision by the time it
/// gets here).
extern "C" fn run(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let outcome = with_table(|table| {
        let Some(entry) = table.get_mut(&id) else {
            return None;
        };
        entry::with_runtime(|context| {
            entry.stmt.clear_bindings();
            bind_positional(context, &mut entry.stmt, [a0, a1, a2, a3]);
        });
        let ran = entry.stmt.run_ignore_rows().is_ok();
        let _ = entry.stmt.reset();
        ran.then(|| entry.conn_id)
    });
    let Some(conn_id) = outcome else {
        return entry::undefined_value();
    };
    let (changes, last_rowid) = super::database::with_table(|conns| {
        conns
            .get(&conn_id)
            .map(|conn| (conn.conn.changes(), conn.conn.last_insert_rowid()))
            .unwrap_or((0, 0))
    });
    entry::with_runtime(|context| {
        let result = entry::make_object(context);
        let changes_value = entry::make_number(changes as f64);
        entry::put_member(context, result, "changes", changes_value);
        let rowid_value = entry::make_number(last_rowid as f64);
        entry::put_member(context, result, "lastInsertRowid", rowid_value);
        result
    })
}

/// `stmt.get(...)` — the first row, or `undefined`.
extern "C" fn get(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let row = with_table(|table| {
        let Some(entry) = table.get_mut(&id) else {
            return None;
        };
        entry::with_runtime(|context| {
            entry.stmt.clear_bindings();
            bind_positional(context, &mut entry.stmt, [a0, a1, a2, a3]);
        });
        let first = loop {
            match entry.stmt.step() {
                Ok(StepResult::Row) => {
                    let values: Vec<turso_core::Value> =
                        entry.stmt.row().map(|row| row.get_values().cloned().collect()).unwrap_or_default();
                    break Some(values);
                }
                Ok(StepResult::Done) => break None,
                Ok(StepResult::IO) => continue,
                _ => break None,
            }
        };
        let names: Vec<String> = (0..entry.stmt.num_columns())
            .map(|idx| entry.stmt.get_column_name(idx).into_owned())
            .collect();
        let _ = entry.stmt.reset();
        first.map(|values| (names, values))
    });
    entry::with_runtime(|context| match row {
        Some((names, values)) => row_object(context, &names, &values),
        None => entry::undefined_in(context),
    })
}

/// `stmt.all(...)` — every row, fully draining and resetting the statement.
extern "C" fn all(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let rows = with_table(|table| {
        let Some(entry) = table.get_mut(&id) else {
            return None;
        };
        entry::with_runtime(|context| {
            entry.stmt.clear_bindings();
            bind_positional(context, &mut entry.stmt, [a0, a1, a2, a3]);
        });
        let names: Vec<String> = (0..entry.stmt.num_columns())
            .map(|idx| entry.stmt.get_column_name(idx).into_owned())
            .collect();
        let collected = entry.stmt.run_collect_rows().unwrap_or_default();
        let _ = entry.stmt.reset();
        Some((names, collected))
    });
    let Some((names, collected)) = rows else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| {
        // `entry::array_append` is ambient — it calls `with_current` itself —
        // so the array is built as a plain `Vec<u64>` and handed to
        // `make_array_in` whole, rather than appended into one element at a
        // time from inside this borrow.
        let rows: Vec<u64> = collected.iter().map(|values| row_object(context, &names, values)).collect();
        entry::make_array_in(context, rows)
    })
}

/// One row, as a plain object keyed by column name — `setReturnArrays` is
/// accepted (see the module doc) but this is the only shape produced.
fn row_object(context: &mut Context, names: &[String], values: &[turso_core::Value]) -> u64 {
    let object = entry::make_object(context);
    for (name, held) in names.iter().zip(values.iter()) {
        let js_value = value::to_js(context, held);
        entry::put_member(context, object, name, js_value);
    }
    object
}

/// `stmt.columns()`. `database`/`column` (origin schema/column name) are
/// always `null` — `turso_core::Statement` exposes a result column's `name`,
/// its source `table`, and `decltype`, and nothing for
/// `sqlite3_column_origin_name`/`sqlite3_column_database_name`, so this
/// module does not invent one.
extern "C" fn columns(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::undefined_value();
    };
    let described = with_table(|table| {
        table.get(&id).map(|entry| {
            (0..entry.stmt.num_columns())
                .map(|idx| {
                    let name = entry.stmt.get_column_name(idx).into_owned();
                    let table_name = entry.stmt.get_column_table_name(idx).map(|value| value.into_owned());
                    let decl_type = entry.stmt.get_column_decltype(idx);
                    (name, table_name, decl_type)
                })
                .collect::<Vec<_>>()
        })
    });
    let Some(described) = described else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| {
        let rows: Vec<u64> = described
            .into_iter()
            .map(|(name, table_name, decl_type)| {
                let row = entry::make_object(context);
                let name_value = entry::make_string(context, &name);
                entry::put_member(context, row, "name", name_value);
                let table_value = match table_name {
                    Some(text) => entry::make_string(context, &text),
                    None => entry::null_in(context),
                };
                entry::put_member(context, row, "table", table_value);
                let type_value = match decl_type {
                    Some(text) => entry::make_string(context, &text),
                    None => entry::null_in(context),
                };
                entry::put_member(context, row, "type", type_value);
                let none = entry::null_in(context);
                entry::put_member(context, row, "database", none);
                entry::put_member(context, row, "column", none);
                row
            })
            .collect();
        entry::make_array_in(context, rows)
    })
}

/// `source_sql` this statement was prepared from — held for
/// [`super::database::prepare`]'s reporting and for a future `sourceSQL`
/// reader that needs the table rather than the instance property.
#[allow(dead_code)]
pub(super) fn source_sql_of(id: u64) -> Option<String> {
    with_table(|table| table.get(&id).map(|entry| entry.source_sql.clone()))
}
