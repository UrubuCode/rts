//! `FileHandle` — what `fs.promises.open` answers, over one shared
//! prototype, this module's own fd table ([`super::fd`]), and the SAME
//! `*_sync` bodies every other member of this module already runs.
//!
//! # Reuse, not a second implementation
//!
//! Every method below is a call into an existing `fd::*_sync` (fd-based) or
//! `basic::*_sync`/`bytes::*_sync` (path/fd-based) native, wrapped in an
//! already-settled `Promise` — exactly what [`super::promises`] already does
//! for the rest of `fs.promises`, and for the same reason: no operation here
//! is allowed to disagree with its `*Sync` sibling about what a call answers.
//!
//! # `readFile`/`writeFile`: through the path this handle was opened with
//!
//! Real Node's `filehandle.readFile()`/`.writeFile()` go through the open fd
//! and honor wherever its cursor happens to be. This module has no fd-based
//! whole-file reader to reuse — only [`super::basic::read_file_sync`]/
//! `write_file_sync`, which are PATH-based — so `open()` also remembers the
//! path it was given, under `__path`, and these two calls that instead of
//! writing a second whole-file reader over the fd table. The divergence: a
//! `filehandle.write()` that moved the cursor, followed by `.readFile()`,
//! sees the WHOLE file from the start here, not from the moved position real
//! Node would read from. Named rather than hidden, the same way every other
//! divergence in this module is.

use rts_core::entry::{self, Provided};

use super::{basic, fd};

const METHODS: &[(&str, Provided)] = &[
    ("read", read),
    ("write", write),
    ("readFile", read_file),
    ("writeFile", write_file),
    ("close", close),
    ("stat", stat),
    ("truncate", truncate),
    ("sync", sync),
];

fn resolve(value: u64) -> u64 {
    entry::with_runtime(|context| entry::settled(context, value, false))
}

fn settle(value: u64, rejected: bool) -> u64 {
    entry::with_runtime(|context| entry::settled(context, value, rejected))
}

/// Rejects when `sync_result` is `undefined` — the same "answers `undefined`
/// on failure" convention [`super::promises`]'s `answered` reuses.
fn answered(sync_result: u64) -> u64 {
    settle(sync_result, sync_result == entry::undefined_value())
}

fn fd_value(this: u64) -> u64 {
    entry::get_indexed(this, super::string("fd"))
}

fn path_value(this: u64) -> u64 {
    entry::get_indexed(this, super::string("__path"))
}

/// `fs.promises.open(path, flags?)`.
pub(super) extern "C" fn open(_e: u64, _this: u64, path: u64, flags: u64, _a2: u64, _a3: u64) -> u64 {
    let handle_fd = fd::open_sync(0, 0, path, flags, 0, 0);
    let absent = entry::undefined_value();
    if handle_fd == absent {
        return settle(absent, true);
    }
    let instance = entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "FileHandle", METHODS);
        let instance = entry::make_instance(context, prototype);
        entry::put_member(context, instance, "fd", handle_fd);
        entry::put_member(context, instance, "__path", path);
        instance
    });
    resolve(instance)
}

/// `filehandle.read(buffer, options?)` — resolves `{ bytesRead, buffer }`,
/// same shape Node's does; `buffer` is the same `Uint8Array`/`DataView`
/// [`super::bytes::read_sync`] filled, handed back rather than copied again.
extern "C" fn read(_e: u64, this: u64, buffer: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    let read = super::bytes::read_sync(0, 0, fd_value(this), buffer, options, 0);
    let absent = entry::undefined_value();
    if read == absent {
        return settle(absent, true);
    }
    let result = entry::with_runtime(|context| {
        let object = entry::make_object(context);
        entry::put_member(context, object, "bytesRead", read);
        entry::put_member(context, object, "buffer", buffer);
        object
    });
    resolve(result)
}

/// `filehandle.write(buffer, options?)` — resolves `{ bytesWritten, buffer }`.
extern "C" fn write(_e: u64, this: u64, buffer: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    let written = super::bytes::write_sync(0, 0, fd_value(this), buffer, options, 0);
    let absent = entry::undefined_value();
    if written == absent {
        return settle(absent, true);
    }
    let result = entry::with_runtime(|context| {
        let object = entry::make_object(context);
        entry::put_member(context, object, "bytesWritten", written);
        entry::put_member(context, object, "buffer", buffer);
        object
    });
    resolve(result)
}

/// `filehandle.readFile(encoding?)` — see the module doc for why this is
/// path-based rather than fd-based.
extern "C" fn read_file(_e: u64, this: u64, encoding: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    answered(basic::read_file_sync(0, 0, path_value(this), encoding, 0, 0))
}

/// `filehandle.writeFile(data)` — see the module doc for why this is
/// path-based rather than fd-based.
extern "C" fn write_file(_e: u64, this: u64, data: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    basic::write_file_sync(0, 0, path_value(this), data, 0, 0);
    settle(entry::undefined_value(), !super::succeeded())
}

/// `filehandle.close()`.
extern "C" fn close(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    fd::close_sync(0, 0, fd_value(this), 0, 0, 0);
    resolve(entry::undefined_value())
}

/// `filehandle.stat()` — the same [`super::stats`] object `fstatSync` gives.
extern "C" fn stat(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    answered(fd::fstat_sync(0, 0, fd_value(this), 0, 0, 0))
}

/// `filehandle.truncate(len?)`.
extern "C" fn truncate(_e: u64, this: u64, len: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    fd::ftruncate_sync(0, 0, fd_value(this), len, 0, 0);
    resolve(entry::undefined_value())
}

/// `filehandle.sync()` — `fdatasync`/`sync` are the same call here (see
/// [`super::fd::fdatasync_sync`]'s own doc for why), so this reuses `fsync`.
extern "C" fn sync(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    fd::fsync_sync(0, 0, fd_value(this), 0, 0, 0);
    resolve(entry::undefined_value())
}
