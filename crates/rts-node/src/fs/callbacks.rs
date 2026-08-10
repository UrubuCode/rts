//! `readFile`/`writeFile`/`stat`/`readdir`/`exists`/`access` — the err-first
//! callback forms of `node:fs`.
//!
//! # This is synchronous work that calls back before returning
//!
//! There is no background I/O anywhere in this engine (see the parent module
//! doc), so every member here does the SAME work its `*Sync` sibling does and
//! then calls `callback` directly — via [`entry::call`], on the JS thread,
//! before the native returns — rather than posting anything to run later.
//! `fs/watch.rs`'s `pump` already establishes that a native calling `.emit`
//! straight through `entry::call` is safe; this is the same call, aimed at an
//! argument instead of a stored listener. The parent module's doc calls the
//! bare callback forms "not implemented" for exactly the reason a STREAM or a
//! repeatedly-invoked `EventEmitter` handler is not — those need re-entry
//! across more than one native call, spread over the object's lifetime. A
//! single err-first callback does not: it is invoked exactly once, inside the
//! one native call that already has the answer.
//!
//! # Reuse: every op is the `*Sync`/promise body's own logic, not re-typed
//!
//! `readFile` reads through [`super::encoding::encode`], the same table
//! `basic::read_file_sync` uses. `stat` reads through
//! [`super::stats::stat_result`], the same non-throwing fetch
//! `fs.promises.stat` uses (see that function's own doc for why the
//! THROWING `stat_sync` is never called from here either — the same rule 8
//! concern: a native cannot ask whether a callee it does not check left a
//! throw behind, and `stat_result` never raises one to ask about).

use rts_core::entry::{self, with_runtime};

/// `callback(a0)` — for `exists`, whose Node callback carries no error slot.
fn invoke1(callback: u64, a0: u64) {
    let absent = entry::undefined_value();
    entry::call(callback, absent, a0, absent, absent, absent);
}

/// `callback(err, data)` — every err-first form here.
fn invoke2(callback: u64, err: u64, data: u64) {
    let absent = entry::undefined_value();
    entry::call(callback, absent, err, data, absent, absent);
}

fn node_error(code: &str, message: &str) -> u64 {
    with_runtime(|context| super::node_error(context, code, message))
}

/// `fs.readFile(path, encoding, callback)` — only the 3-argument form the
/// test suite calls; `fs.readFile(path, callback)` (no encoding) is not
/// distinguished from a caller passing `encoding` as `undefined`, which reads
/// as `"utf8"` the same way every `*Sync` sibling's absent encoding does, so
/// the 2-argument form still works, just not via a shift check.
pub(super) extern "C" fn read_file(_e: u64, _this: u64, path: u64, encoding: u64, callback: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let (encoding, callback) = if callback == absent && with_runtime(|context| entry::is_callable_in(context, encoding)) {
        (absent, encoding)
    } else {
        (encoding, callback)
    };
    let Some(path_text) = super::text(path) else {
        invoke2(callback, node_error("ENOENT", "ENOENT: no such file or directory, open"), absent);
        return absent;
    };
    match std::fs::read(&path_text) {
        Ok(bytes) => {
            let name = super::text(encoding).unwrap_or_else(|| "utf8".to_string());
            let data = match super::encoding::encode(&name, &bytes) {
                Some(text) => super::string(&text),
                None => absent,
            };
            invoke2(callback, entry::null_value(), data);
        }
        Err(io_error) => {
            let code = super::io_node_code(io_error.kind());
            let message = format!("{code}: no such file or directory, open '{path_text}'");
            invoke2(callback, node_error(code, &message), absent);
        }
    }
    absent
}

/// `fs.writeFile(path, data, callback)` — the 3-argument form the test suite
/// calls; a 4th `options` slot is accepted and ignored (this crate's
/// `*Sync` sibling already reads no more than `encoding`, itself not
/// exercised by any writer here yet).
pub(super) extern "C" fn write_file(_e: u64, _this: u64, path: u64, data: u64, callback: u64, a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let callback = if callback == absent { a3 } else { callback };
    let (Some(path_text), Some(data_text)) = (super::text(path), super::text(data)) else {
        invoke2(callback, node_error("ENOENT", "ENOENT: no such file or directory, open"), absent);
        return absent;
    };
    match std::fs::write(&path_text, data_text) {
        Ok(()) => invoke1(callback, entry::null_value()),
        Err(io_error) => {
            let code = super::io_node_code(io_error.kind());
            let message = format!("{code}: no such file or directory, open '{path_text}'");
            invoke2(callback, node_error(code, &message), absent);
        }
    }
    absent
}

/// `fs.stat(path, callback)`.
pub(super) extern "C" fn stat(_e: u64, _this: u64, path: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(path_text) = super::text(path) else {
        invoke2(callback, node_error("ENOENT", "ENOENT: no such file or directory, stat"), absent);
        return absent;
    };
    match super::stats::stat_result(&path_text, true) {
        Ok(value) => invoke2(callback, entry::null_value(), value),
        Err(io_error) => {
            let code = super::io_node_code(io_error.kind());
            let message = format!("{code}: no such file or directory, stat '{path_text}'");
            invoke2(callback, node_error(code, &message), absent);
        }
    }
    absent
}

/// `fs.exists(path, callback)` — Node's one callback with no error slot:
/// `callback(boolean)`.
pub(super) extern "C" fn exists(_e: u64, _this: u64, path: u64, callback: u64, _a2: u64, _a3: u64) -> u64 {
    let found = super::text(path).is_some_and(|path| std::path::Path::new(&path).exists());
    invoke1(callback, entry::boolean_value(found));
    entry::undefined_value()
}

/// `fs.access(path, callback)` — Node's 3-argument `(path, mode, callback)`
/// form is accepted by shifting the same way [`read_file`] shifts `encoding`.
pub(super) extern "C" fn access(_e: u64, _this: u64, path: u64, mode: u64, a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let callback = if a2 != absent { a2 } else { mode };
    let found = super::text(path).is_some_and(|path| std::path::Path::new(&path).exists());
    match found {
        true => invoke2(callback, entry::null_value(), absent),
        false => invoke2(callback, node_error("ENOENT", "ENOENT: no such file or directory, access"), absent),
    }
    absent
}

/// `fs.readdir(path, callback)`.
pub(super) extern "C" fn readdir(_e: u64, _this: u64, path: u64, callback: u64, a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let callback = if with_runtime(|context| entry::is_callable_in(context, callback)) { callback } else { a2 };
    let Some(path_text) = super::text(path) else {
        invoke2(callback, node_error("ENOENT", "ENOENT: no such file or directory, scandir"), absent);
        return absent;
    };
    match std::fs::read_dir(&path_text) {
        Ok(entries) => {
            let names: Vec<u64> = entries
                .filter_map(|entry| entry.ok())
                .map(|entry| super::string(&entry.file_name().to_string_lossy()))
                .collect();
            let array = entry::make_array(names);
            invoke2(callback, entry::null_value(), array);
        }
        Err(io_error) => {
            let code = super::io_node_code(io_error.kind());
            let message = format!("{code}: no such file or directory, scandir '{path_text}'");
            invoke2(callback, node_error(code, &message), absent);
        }
    }
    absent
}
