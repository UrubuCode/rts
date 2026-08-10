//! `createReadStream`/`createWriteStream`/`ReadStream`/`WriteStream` — real
//! file I/O over `node:stream`'s `Readable`/`Writable` (`crate::stream`),
//! rather than a parallel stream implementation. `install` (see `lib.rs`)
//! runs `stream::namespace` before any program code can reach `fs`, so the
//! `"Readable"`/`"Writable"` prototypes [`entry::make_prototype`] hands back
//! by name here are always the SAME, fully-built ones that module installs —
//! the identity `instanceof Writable`/`instanceof Readable` in the test suite
//! depends on.
//!
//! # `WriteStream`: real, driven through `Writable`'s own `_write` hook
//!
//! `createWriteStream` opens a real file and constructs a genuine
//! `stream.Writable` (via [`entry::construct`], so every field
//! `writable::init` sets up exists) whose `_write` is [`write_hook`] — a
//! native that looks the instance's OPEN FILE up in [`WRITE_TABLE`], keyed by
//! an id on the instance (`fs/watch.rs`'s `__watchId` pattern), and writes
//! for real. `Writable`'s own `end`/`finish`/backpressure machinery is not
//! reimplemented; `write_hook` only has to call its `callback` once the real
//! write lands, which `stream/writable.rs`'s own doc says is what "complete"
//! means here.
//!
//! # `ReadStream`: content read lazily, on the first thing that would flow it
//!
//! `stream/mod.rs`'s own doc names the rule this depends on: THIS crate's
//! `Readable` only promotes to flowing on `.pipe()`/`.resume()`, never merely
//! on `.on('data', …)` — real Node's own `Readable` overrides `on` to auto-
//! resume there, and [`ON_OVERRIDE`]/[`PIPE_OVERRIDE`] are that override,
//! reached through `ReadStream.prototype` shadowing the inherited one. Content
//! is not pushed at construction time: it is read (and, per `setEncoding`,
//! decoded) the first time [`ensure_started`] runs, which happens from
//! whichever of `on("data", …)`/`.pipe()` runs first — so a caller that calls
//! `setEncoding` before either (as this crate's own tests do) has it take
//! effect, matching `readable::push`'s own encode-at-push-time behaviour.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rts_core_rwk::entry::{self, Context, Provided};

struct WriteEntry {
    file: std::fs::File,
}

static WRITE_TABLE: Mutex<Option<HashMap<u64, WriteEntry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_write_table<T>(body: impl FnOnce(&mut HashMap<u64, WriteEntry>) -> T) -> T {
    let mut guard = WRITE_TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

fn stream_member(context: &mut Context, name: &str) -> u64 {
    let namespace = crate::stream::namespace(context);
    entry::get_member(context, namespace, name)
}

fn id_of(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, super::string("__streamId"));
    entry::number_of(value).map(|value| value as u64)
}

/// A fresh, empty-bodied prototype chained onto `parent`'s real one, plus
/// `methods` — the same recipe `fs/watch.rs::chained_prototype` and
/// `stream/common.rs::chained_prototype` both use, generalised to take an
/// arbitrary named parent (`"Writable"`/`"Readable"`, not `"EventEmitter"`).
fn chained(context: &mut Context, parent: &'static str, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    let parent_prototype = entry::make_prototype(context, parent, &[]);
    let prototype = entry::make_prototype(context, name, methods);
    entry::set_prototype_in(context, prototype, parent_prototype);
    prototype
}

// --------------------------------------------------------------- WriteStream

/// `_write(chunk, encoding, callback)` — the real file write. `chunk` is
/// either the original JS string (`Writable`'s own `normalize_chunk` leaves a
/// default-`"utf8"` string alone) or a `Uint8Array` (any other canonical
/// encoding); both are read the same way `bytes.rs`'s own members do.
extern "C" fn write_hook(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(id) = id_of(this) else {
        entry::call(callback, absent, absent, absent, absent, absent);
        return absent;
    };
    let bytes: Vec<u8> = match entry::text_of(chunk) {
        Some(text) => text.into_bytes(),
        None => entry::with_runtime(|context| entry::bytes_of(context, chunk)).unwrap_or_default(),
    };
    let length = bytes.len();
    let ok = with_write_table(|table| table.get_mut(&id).map(|entry| entry.file.write_all(&bytes).is_ok())).unwrap_or(false);
    if ok {
        let written = entry::with_runtime(|context| entry::get_member(context, this, "bytesWritten"));
        let total = entry::number_of(written).unwrap_or(0.0) + length as f64;
        entry::with_runtime(|context| {
            let value = entry::make_number(total);
            entry::put_member(context, this, "bytesWritten", value);
        });
    }
    entry::call(callback, absent, absent, absent, absent, absent);
    absent
}

fn open_for_write(path: &str, flags: Option<&str>) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    match flags {
        Some("a") | Some("a+") => options.create(true).append(true),
        _ => options.create(true).write(true).truncate(true),
    };
    options.open(path)
}

/// `fs.createWriteStream(path, options?)`. `options.flags` (only `"a"`
/// checked, matching Node's own append/truncate split) is the sole option
/// read — `start`, `fs` (a custom binding table), `autoClose: false` and a
/// pre-opened `fd` are not: none of this crate's other stream tests exercise
/// them, and each needs either a second open path or a table this module has
/// no other reason to keep.
pub(super) extern "C" fn create_write_stream(_e: u64, _this: u64, path: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(path_text) = super::text(path) else {
        return absent;
    };
    let flags = entry::with_runtime(|context| entry::get_member(context, options, "flags"));
    let flags_text = entry::text_of(flags);
    let Ok(file) = open_for_write(&path_text, flags_text.as_deref()) else {
        return absent;
    };
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_write_table(|table| table.insert(id, WriteEntry { file }));
    // Several SEPARATE `with_runtime` calls, never nested: `entry::construct`
    // is ambient (it opens its own borrow via `with_current`), and calling it
    // from inside an outer `with_runtime` closure is the same re-entrant-borrow
    // abort `fs/watch.rs`'s module doc names for `set_prototype`.
    let (writable_ctor, write_options) = entry::with_runtime(|context| {
        let writable_ctor = stream_member(context, "Writable");
        let write_options = entry::make_object(context);
        let hook = entry::make_callable(context, write_hook);
        entry::put_member(context, write_options, "write", hook);
        (writable_ctor, write_options)
    });
    let instance = entry::construct(writable_ctor, write_options, absent, absent, absent);
    entry::with_runtime(|context| {
        let write_stream_prototype = chained(context, "Writable", "fs.WriteStream", &[]);
        entry::set_prototype_in(context, instance, write_stream_prototype);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__streamId", id_value);
        let path_value = entry::make_string(context, &path_text);
        entry::put_member(context, instance, "path", path_value);
        let bytes_written = entry::make_number(0.0);
        entry::put_member(context, instance, "bytesWritten", bytes_written);
    });
    instance
}

/// `new fs.WriteStream(...)` — never called directly by anything this crate
/// tests (only used as the exported value `instanceof` checks against, and
/// its `.prototype` is what [`create_write_stream`] chains instances onto);
/// answers `undefined` rather than building a half-set-up instance if it
/// somehow is.
extern "C" fn write_stream_construct(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}

// ---------------------------------------------------------------- ReadStream

/// Reads `this.path` (a plain data property, not a table lookup — see the
/// module doc for why `ReadStream`, unlike `WriteStream`, does not need one:
/// nothing here keeps a file open across calls) and pushes it, decoded per
/// `readableEncoding` if `setEncoding` was already called, then signals EOF.
/// Idempotent via `__started__`, since both [`on_override`] (on `"data"`) and
/// [`pipe_override`] call this, and a caller doing both must not push twice.
fn ensure_started(this: u64) {
    let absent = entry::undefined_value();
    let started = entry::with_runtime(|context| entry::get_member(context, this, "__started__"));
    if started == entry::boolean_value(true) {
        return;
    }
    entry::with_runtime(|context| {
        let flag = entry::boolean_value(true);
        entry::put_member(context, this, "__started__", flag);
    });
    let path = entry::with_runtime(|context| entry::get_member(context, this, "path"));
    let Some(path_text) = entry::text_of(path) else {
        return;
    };
    let Ok(bytes) = std::fs::read(&path_text) else {
        return;
    };
    let chunk = entry::with_runtime(|context| entry::make_bytes(context, &bytes));
    let push_fn = entry::with_runtime(|context| entry::get_member(context, this, "push"));
    entry::call(push_fn, this, chunk, absent, absent, absent);
    entry::call(push_fn, this, entry::null_value(), absent, absent, absent);
}

/// The REAL `Readable.prototype`'s own `name` member — reached directly off
/// the named prototype rather than off `this` (which finds THIS module's own
/// override, since `ReadStream.prototype` shadows it; calling that back would
/// recurse into [`on_override`]/[`pipe_override`] forever).
fn readable_member(name: &str) -> u64 {
    entry::with_runtime(|context| {
        let readable_prototype = entry::make_prototype(context, "Readable", &[]);
        entry::get_member(context, readable_prototype, name)
    })
}

/// `readStream.on(event, listener)` — [`ensure_started`] before delegating to
/// the REAL `Readable.prototype.on` (via [`readable_member`], not `this`'s
/// own — `this`'s is this override, and calling it back would recurse) when
/// `event === "data"`, matching real Node's auto-resume; every other event
/// name is a plain passthrough.
extern "C" fn on_override(_e: u64, this: u64, event: u64, listener: u64, c: u64, d: u64) -> u64 {
    let absent = entry::undefined_value();
    let real_on = readable_member("on");
    entry::call(real_on, this, event, listener, c, d);
    if entry::text_of(event).as_deref() == Some("data") {
        ensure_started(this);
        let resume_fn = entry::with_runtime(|context| entry::get_member(context, this, "resume"));
        entry::call(resume_fn, this, absent, absent, absent, absent);
    }
    this
}

/// `readStream.pipe(destination, options?)` — [`ensure_started`], then the
/// real `Readable.prototype.pipe`, which itself calls `resume()`.
extern "C" fn pipe_override(_e: u64, this: u64, destination: u64, options: u64, c: u64, d: u64) -> u64 {
    ensure_started(this);
    let real_pipe = readable_member("pipe");
    entry::call(real_pipe, this, destination, options, c, d)
}

const READ_STREAM_METHODS: &[(&str, Provided)] = &[("on", on_override), ("addListener", on_override), ("pipe", pipe_override)];

/// `fs.createReadStream(path, options?)`. `start`/`end`/`fd`/`autoClose` are
/// not read — see [`create_write_stream`]'s doc for the same call on the
/// write side; content is always the whole file, per [`ensure_started`].
pub(super) extern "C" fn create_read_stream(_e: u64, _this: u64, path: u64, _options: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(path_text) = super::text(path) else {
        return absent;
    };
    let readable_ctor = entry::with_runtime(|context| stream_member(context, "Readable"));
    let instance = entry::construct(readable_ctor, absent, absent, absent, absent);
    entry::with_runtime(|context| {
        let read_stream_prototype = chained(context, "Readable", "fs.ReadStream", READ_STREAM_METHODS);
        entry::set_prototype_in(context, instance, read_stream_prototype);
        let path_value = entry::make_string(context, &path_text);
        entry::put_member(context, instance, "path", path_value);
        let pending = entry::boolean_value(true);
        entry::put_member(context, instance, "pending", pending);
        let started = entry::boolean_value(false);
        entry::put_member(context, instance, "__started__", started);
    });
    instance
}

extern "C" fn read_stream_construct(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}

// -------------------------------------------------------------- registration

/// The `WriteStream`/`ReadStream` constructors `node:fs` exports — see
/// [`write_stream_construct`]/[`read_stream_construct`] for why calling
/// either directly answers `undefined` rather than building a real instance.
///
/// # Deliberately does NOT chain onto `"Writable"`/`"Readable"` here
///
/// `lib.rs::install`'s own comment names the trap this would otherwise be:
/// `make_prototype` is idempotent BY NAME, and whoever asks for a name FIRST
/// decides what is on it. `install` calls `fs::namespace` (this function's
/// caller, transitively) BEFORE `stream::namespace` — so a call here to
/// `make_prototype(context, "Writable", …)` would register "Writable" with
/// NO methods, and the real one `stream::namespace` builds moments later
/// would never take: `new Writable(...).write` would be `undefined`
/// EVERYWHERE in the program, not just on a `fs.WriteStream`. This builds
/// `WriteStream`/`ReadStream`'s own prototypes UNLINKED; [`create_write_stream`]/
/// [`create_read_stream`] do the actual chaining onto `Writable`/`Readable`,
/// every time they run — always well after `install` has finished, so
/// `"Writable"`/`"Readable"` are the real, already-built ones by then.
pub(super) fn constructors(context: &mut Context) -> [(&'static str, u64); 2] {
    // "fs.WriteStream"/"fs.ReadStream" rather than the bare names: `node:tty`
    // registers ITS OWN `"WriteStream"`/`"ReadStream"` under those same bare
    // names, with a different method table, and `make_prototype` is idempotent
    // by name — see its doc comment for the collision this otherwise is.
    let write_stream_prototype = entry::make_prototype(context, "fs.WriteStream", &[]);
    let write_stream_ctor = entry::make_callable(context, write_stream_construct);
    entry::put_member(context, write_stream_ctor, "prototype", write_stream_prototype);

    let read_stream_prototype = entry::make_prototype(context, "fs.ReadStream", READ_STREAM_METHODS);
    let read_stream_ctor = entry::make_callable(context, read_stream_construct);
    entry::put_member(context, read_stream_ctor, "prototype", read_stream_prototype);

    [("WriteStream", write_stream_ctor), ("ReadStream", read_stream_ctor)]
}
