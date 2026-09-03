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

use super::{basic, bytes, fd, streams};

const METHODS: &[(&str, Provided)] = &[
    ("read", read),
    ("write", write),
    ("readFile", read_file),
    ("writeFile", write_file),
    ("close", close),
    ("stat", stat),
    ("truncate", truncate),
    ("sync", sync),
    ("createReadStream", create_read_stream),
    ("createWriteStream", create_write_stream),
    ("readableWebStream", readable_web_stream),
    ("readLines", read_lines),
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

// -------------------------------------------------------- stream factories
//
// The four members below all go through EXISTING machinery — the streams
// [`super::streams`] already builds, the async iterator recipe
// [`super::super::buffer::blob`]'s `stream()` established for a WHATWG
// `ReadableStream`, and [`super::super::readline`]'s own line-splitter and
// `Interface` prototype — over the PATH this handle was opened with, per this
// module's own doc on why `readFile`/`writeFile` are path- rather than
// fd-based. None of them is a second stream implementation.

/// `filehandle.createReadStream(options?)` — [`streams::create_read_stream`],
/// over `this` handle's own path.
extern "C" fn create_read_stream(e: u64, this: u64, options: u64, a2: u64, a3: u64, _a4: u64) -> u64 {
    streams::create_read_stream(e, this, path_value(this), options, a2, a3)
}

/// `filehandle.createWriteStream(options?)` — [`streams::create_write_stream`],
/// over `this` handle's own path.
extern "C" fn create_write_stream(e: u64, this: u64, options: u64, a2: u64, a3: u64, _a4: u64) -> u64 {
    streams::create_write_stream(e, this, path_value(this), options, a2, a3)
}

/// Where [`readable_web_stream`] leaves the one chunk [`web_stream_start`]
/// enqueues — an own property of the `source` object `ReadableStream`'s
/// constructor is handed, never of the handle itself.
const CHUNK: &str = "__chunk__";

/// `filehandle.readableWebStream()` — a WHATWG `ReadableStream` over the
/// whole file, read eagerly. Same recipe as `buffer/blob.rs::stream_method`
/// (global lookup, one `start(controller)` that enqueues-then-closes) since a
/// `Blob`'s bytes are resident there for the identical reason a file's are
/// here once read — not called directly because that function reads a
/// `Blob`'s own byte table, which a `FileHandle` has no entry in.
extern "C" fn readable_web_stream(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let class = global("ReadableStream");
    if !entry::with_runtime(|context| entry::is_callable_in(context, class)) {
        return absent;
    }
    let read = bytes::read_file_sync(0, 0, path_value(this), absent, 0, 0);
    let source = entry::with_runtime(|context| {
        let source = entry::make_object(context);
        if read != absent {
            entry::put_member(context, source, CHUNK, read);
        }
        let start = entry::make_callable(context, web_stream_start);
        entry::put_member(context, source, "start", start);
        source
    });
    entry::construct(class, source, absent, absent, absent)
}

/// The `start(controller)` [`readable_web_stream`]'s source carries. `this`
/// is that source. See `buffer/blob.rs::stream_start`'s doc for why both
/// `thrown()` questions matter: a controller whose `enqueue` raised must not
/// then be told to `close`.
extern "C" fn web_stream_start(_e: u64, this: u64, controller: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let chunk = entry::get_indexed(this, name_value(CHUNK));
    if chunk != absent {
        let enqueue = entry::get_indexed(controller, name_value("enqueue"));
        entry::call(enqueue, controller, chunk, absent, absent, absent);
        if entry::thrown() != 0 {
            return absent;
        }
    }
    let close = entry::get_indexed(controller, name_value("close"));
    entry::call(close, controller, absent, absent, absent, absent);
    absent
}

/// One global by name, `undefined` when nothing installed it.
fn global(name: &str) -> u64 {
    let key = entry::with_runtime(|context| i64::from(entry::member_key(context, name)));
    entry::global_get(key)
}

/// A property name as a value, for the ambient `get_indexed`.
fn name_value(name: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, name))
}

/// Where [`read_lines`] keeps the already-split lines, and the cursor
/// [`read_lines_next`] advances over them.
const LINES: &str = "__lines__";
const AT: &str = "__at__";
/// Set once `.on("line", …)` has replayed every line to a listener, so a
/// caller doing both that and `for await` does not see them twice — the same
/// idiom `streams.rs::ensure_started` uses for `"data"`.
const STARTED: &str = "__started__";

const READ_LINES_METHODS: &[(&str, Provided)] = &[
    ("on", read_lines_on),
    ("addListener", read_lines_on),
    ("@@iterator", read_lines_self),
    ("next", read_lines_next),
];

/// `filehandle.readLines(options?)` — the file's lines, split the same way
/// [`crate::readline`]'s own `'data'` listener does
/// ([`crate::readline::split_lines`]), over an instance chained onto the REAL
/// `readline.Interface` prototype.
///
/// `crate::readline::namespace` is forced here rather than assumed: it is on
/// this crate's LAZY-module list (`lib.rs`), so nothing guarantees a program
/// that never wrote `import "node:readline"` has already built it — and
/// `make_prototype`'s "idempotent by name" contract means winning the name
/// with an empty table first would leave `close`/`pause`/`resume`/`emit`/`on`
/// permanently missing (see `entry::make_prototype`'s own doc on the
/// collision panic this avoids by staying inside the SAME file — `mod.rs`'s
/// `channel_constructor` states the identical reasoning for
/// `node:diagnostics_channel`). `options.encoding` reads the same as
/// `readFileSync`'s; nothing else in `options` does.
extern "C" fn read_lines(_e: u64, this: u64, options: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let encoding = entry::with_runtime(|context| entry::get_member(context, options, "encoding"));
    let text_value = basic::read_file_sync(0, 0, path_value(this), encoding, 0, 0);
    let lines = match entry::text_of(text_value) {
        Some(text) => crate::readline::split_lines(&text).0,
        None => Vec::new(),
    };
    entry::with_runtime(|context| {
        crate::readline::namespace(context);
        let prototype = streams::chained(context, "Interface", "fs.FileHandle.ReadLines", READ_LINES_METHODS);
        let instance = entry::make_instance(context, prototype);
        let mut values = Vec::with_capacity(lines.len());
        for line in &lines {
            values.push(entry::make_string(context, line));
        }
        let array = entry::make_array_in(context, values);
        entry::put_member(context, instance, LINES, array);
        entry::put_member(context, instance, AT, entry::make_number(0.0));
        entry::put_member(context, instance, STARTED, entry::boolean_value(false));
        instance
    })
}

/// One member of the REAL `Interface.prototype` — reached directly off that
/// named prototype (chain-read, empty table) rather than off `this` (which
/// would find THIS module's own [`read_lines_on`] override and recurse).
/// Mirrors `streams.rs::readable_member` for `"Readable"`.
fn interface_member(name: &str) -> u64 {
    entry::with_runtime(|context| {
        let interface_prototype = entry::make_prototype(context, "Interface", &[]);
        entry::get_member(context, interface_prototype, name)
    })
}

/// `rl.on(event, listener)` — the real, inherited `on` first (so the listener
/// is genuinely recorded and every other event keeps working), and when
/// `event === "line"`, every already-split line is delivered to it right
/// away: there is no live stream here to await, the same eager-on-subscribe
/// shape `streams.rs::on_override` uses for `"data"`.
extern "C" fn read_lines_on(_e: u64, this: u64, event: u64, listener: u64, c: u64, d: u64) -> u64 {
    let real_on = interface_member("on");
    entry::call(real_on, this, event, listener, c, d);
    if entry::text_of(event).as_deref() == Some("line") {
        deliver_lines(this, listener);
    }
    this
}

/// Every line, in order, to `listener` — once. Collected under one borrow and
/// called from outside it, the rule every native calling into JS here follows.
fn deliver_lines(this: u64, listener: u64) {
    let (already, array) = entry::with_runtime(|context| {
        let flag = entry::get_member(context, this, STARTED);
        let already = entry::to_boolean_in(context, flag);
        entry::put_member(context, this, STARTED, entry::boolean_value(true));
        (already, entry::get_member(context, this, LINES))
    });
    if already {
        return;
    }
    let absent = entry::undefined_value();
    let length = entry::array_length(array) as u64;
    for index in 0..length {
        let value = entry::element_at(array, entry::make_number(index as f64));
        entry::call(listener, absent, value, absent, absent, absent);
    }
}

/// `rl[Symbol.iterator]()` — the object itself, the same
/// `[Symbol.asyncIterator]() { return this }` shape
/// `timers/promises.rs::iterator_self` uses for its own iterator.
extern "C" fn read_lines_self(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    this
}

/// `rl.next()` — the sync iterator protocol `for await` falls back to when a
/// subject has no `[Symbol.asyncIterator]` (`rts-codegen`'s
/// `emit/for_await.rs`): an `{ value, done }` object, advancing `__at__` by
/// one until the line array is exhausted.
///
/// Three steps, not one `with_runtime`: [`entry::array_length`]/
/// [`entry::element_at`] are AMBIENT (they open their own borrow via
/// `with_current`), so calling either one from inside an already-open
/// `with_runtime` closure aborts the process with "RefCell already
/// borrowed" — this crate's whole calling convention exists to keep the two
/// apart, and this function is where the fix landed after that abort was
/// reproduced.
extern "C" fn read_lines_next(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (array, at) = entry::with_runtime(|context| {
        let array = entry::get_member(context, this, LINES);
        let at = entry::number_of(entry::get_member(context, this, AT)).unwrap_or(0.0) as u64;
        (array, at)
    });
    let length = entry::array_length(array) as u64;
    if at >= length {
        return entry::with_runtime(|context| {
            let result = entry::make_object(context);
            let absent = entry::undefined_in(context);
            entry::put_member(context, result, "value", absent);
            entry::put_member(context, result, "done", entry::boolean_value(true));
            result
        });
    }
    let value = entry::element_at(array, entry::make_number(at as f64));
    entry::with_runtime(|context| {
        let result = entry::make_object(context);
        entry::put_member(context, result, "value", value);
        entry::put_member(context, result, "done", entry::boolean_value(false));
        let advanced = entry::make_number((at + 1) as f64);
        entry::put_member(context, this, AT, advanced);
        result
    })
}
