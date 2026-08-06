//! `openSync`/`closeSync`/`fstatSync`/`fsyncSync`/`fdatasyncSync`/
//! `ftruncateSync` — the fd-based members this module can answer without a
//! `Buffer` (see the module doc for why `readSync`/`writeSync` cannot be).
//!
//! # The fd table is this module's own
//!
//! A fd here is a key into [`TABLE`], not an OS file descriptor number —
//! `std::fs::File` does not expose a stable small integer on every platform
//! the way POSIX `open(2)` does, and this module's own table sidesteps that
//! without reaching for platform-specific `AsRawFd`/`AsRawHandle` code the
//! rest of this module does not otherwise need. Every member that takes a
//! fd here only ever means "a fd this module's own `openSync` handed out".

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use super::text;

/// The open-file table `openSync` allocates into and every other fd-based
/// member here reads.
pub(super) static TABLE: Mutex<Option<HashMap<i64, std::fs::File>>> = Mutex::new(None);
/// The first number this table hands out.
///
/// Three, not one: 0, 1 and 2 are stdin, stdout and stderr everywhere a
/// descriptor means anything, and Node's own `fs.writeSync(1, …)` writes to
/// standard output. Starting at 1 would make this table's first open FILE
/// answer the number a program uses to mean stdout — a collision that costs
/// nothing to avoid and would be found the hard way once `writeSync` lands.
static NEXT_FD: AtomicI64 = AtomicI64::new(3);

fn with_table<T>(body: impl FnOnce(&mut HashMap<i64, std::fs::File>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

/// `fs.openSync(path, flags?, mode?)` — a subset of Node's flag strings:
/// `'r'`, `'r+'`, `'w'`, `'wx'`, `'w+'`, `'a'`, `'a+'`. `mode` (the
/// create-permission bits) is not applied — POSIX permission bits on create
/// need the same `libc`/`PermissionsExt` reach as `chmodSync`, already
/// covered for an EXISTING path there; wiring it into `OpenOptions` at
/// create time is the one piece not reused from that.
///
/// `undefined` on failure, not a negative "errno" number: a negative fd is a
/// plausible-looking wrong value the same way `""` would be for a read.
pub(super) extern "C" fn open_sync(_e: u64, _this: u64, path: u64, flags: u64, _mode: u64, _a3: u64) -> u64 {
    let Some(path) = text(path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let flags = text(flags).unwrap_or_else(|| "r".to_string());
    let mut options = std::fs::OpenOptions::new();
    match flags.as_str() {
        "r" => {
            options.read(true);
        }
        "r+" => {
            options.read(true).write(true);
        }
        "w" => {
            options.write(true).create(true).truncate(true);
        }
        "wx" => {
            options.write(true).create_new(true);
        }
        "w+" => {
            options.read(true).write(true).create(true).truncate(true);
        }
        "wx+" => {
            options.read(true).write(true).create_new(true);
        }
        "a" => {
            options.append(true).create(true);
        }
        "ax" => {
            options.append(true).create_new(true);
        }
        "a+" => {
            options.read(true).append(true).create(true);
        }
        "ax+" => {
            options.read(true).append(true).create_new(true);
        }
        _ => {
            options.read(true);
        }
    }
    let Ok(file) = options.open(&path) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let fd = NEXT_FD.fetch_add(1, Ordering::SeqCst);
    with_table(|table| table.insert(fd, file));
    rts_core_rwk::entry::make_number(fd as f64)
}

/// `fs.closeSync(fd)`.
pub(super) extern "C" fn close_sync(_e: u64, _this: u64, fd: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(fd) = super::number(fd) {
        with_table(|table| {
            table.remove(&(fd as i64));
        });
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.fstatSync(fd)` — same `Stats`-shaped object [`super::stats::build`]
/// gives the path-based forms.
pub(super) extern "C" fn fstat_sync(_e: u64, _this: u64, fd: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(fd) = super::number(fd) else {
        return rts_core_rwk::entry::undefined_value();
    };
    let metadata = with_table(|table| table.get(&(fd as i64)).and_then(|file| file.metadata().ok()));
    match metadata {
        Some(metadata) => super::stats::build(&metadata, false),
        None => rts_core_rwk::entry::undefined_value(),
    }
}

/// `fs.fsyncSync(fd)`.
pub(super) extern "C" fn fsync_sync(_e: u64, _this: u64, fd: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(fd) = super::number(fd) {
        with_table(|table| {
            if let Some(file) = table.get(&(fd as i64)) {
                let _ = file.sync_all();
            }
        });
    }
    rts_core_rwk::entry::undefined_value()
}

/// `fs.fdatasyncSync(fd)`.
///
/// `std::fs::File` has no data-only sync (`fdatasync(2)`) — only
/// `sync_all`/`sync_data` behind `std::os::unix::fs::FileExt`, and this
/// stays platform-uniform by calling `sync_all` on every platform, which is
/// a strictly-stronger sync than Node's `fdatasyncSync` promises rather than
/// a weaker one — a divergence worth stating, not a wrong answer.
pub(super) extern "C" fn fdatasync_sync(_e: u64, _this: u64, fd: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    fsync_sync(_e, _this, fd, _a1, _a2, _a3)
}

/// `fs.ftruncateSync(fd, len?)` — default `len = 0`.
pub(super) extern "C" fn ftruncate_sync(_e: u64, _this: u64, fd: u64, len: u64, _a2: u64, _a3: u64) -> u64 {
    if let Some(fd) = super::number(fd) {
        let len = super::number(len).unwrap_or(0.0).max(0.0) as u64;
        with_table(|table| {
            if let Some(file) = table.get(&(fd as i64)) {
                let _ = file.set_len(len);
            }
        });
    }
    rts_core_rwk::entry::undefined_value()
}
