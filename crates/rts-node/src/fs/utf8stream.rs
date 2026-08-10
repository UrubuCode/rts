//! `Utf8Stream` — pino's append-only, fixed-encoding file writer
//! (`sonic-boom`'s public shape), backed by real file I/O.
//!
//! Deliberately its OWN class rather than a `Writable` subclass (unlike
//! [`super::streams`]'s `WriteStream`): pino's `Utf8Stream` predates and does
//! not implement the `stream.Writable` contract (no `_write` hook, no
//! backpressure return from `.write()`, a `minLength`/`maxLength` buffering
//! policy `Writable` has no concept of) — chaining onto `Writable` here would
//! be modelling an inheritance relationship real Node's own class does not
//! have.
//!
//! # Buffering, per the options this crate reads
//!
//! `minLength` (default `0`): `.write()` appends to an in-memory buffer and
//! flushes to the real file once the buffer reaches `minLength` bytes — `0`
//! means "flush every write", matching a caller that never opted into
//! batching. `maxLength` (default: no cap): a `.write()` that would push the
//! BUFFERED length past it is dropped whole (never partially written) and
//! `'drop'` fires instead of `'write'` — pino's own behaviour under
//! overflow, named here as "drop the newest" as the one dropping rule that
//! IS observable without a real backpressure signal to make the drop
//! decision on.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rts_core::entry::{self, Context, Provided};

struct Entry {
    file: std::fs::File,
    buffer: String,
    min_length: usize,
    max_length: Option<usize>,
}

static TABLE: Mutex<Option<HashMap<u64, Entry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, Entry>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

fn id_of(this: u64) -> Option<u64> {
    let value = entry::get_indexed(this, super::string("__streamId"));
    entry::number_of(value).map(|value| value as u64)
}

/// `this` if it is already an object (a `new` over a subclass hands one in),
/// else a fresh instance of `prototype` — [`stream::common::self_or_new`]'s
/// own recipe, not reachable from here (private to that module), so repeated
/// rather than reached for over a crate boundary that would buy nothing: it
/// is four lines with no state of its own.
fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

fn chained_prototype(context: &mut Context, methods: &[(&str, Provided)]) -> u64 {
    let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
    let prototype = entry::make_prototype(context, "Utf8Stream", methods);
    entry::set_prototype_in(context, prototype, event_emitter);
    prototype
}

fn emit(this: u64, event: &str, argument: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    let event_key = super::string(event);
    entry::call(emit_fn, this, event_key, argument, absent, absent);
}

const METHODS: &[(&str, Provided)] = &[
    ("write", write),
    ("end", end),
    ("flush", flush),
    ("flushSync", flush),
];

/// `new fs.Utf8Stream(options)` — `options.file` (path), `options.append`
/// (default `true`), `options.minLength`/`options.maxLength` (see the module
/// doc). `options.contentMode`/`mkdir`/`sync`/`fsync`/`retryEAGAIN` are not
/// read: this crate always writes synchronously and creates no missing parent
/// directory, so there is nothing those would change here.
pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let file_value = entry::with_runtime(|context| entry::get_member(context, options, "file"));
    let Some(path_text) = entry::text_of(file_value) else {
        return entry::with_runtime(|context| {
            let prototype = chained_prototype(context, METHODS);
            self_or_new(context, this, prototype)
        });
    };
    let append_value = entry::with_runtime(|context| entry::get_member(context, options, "append"));
    let append = if append_value == absent { true } else { entry::with_runtime(|context| entry::to_boolean_in(context, append_value)) };
    let min_length = entry::with_runtime(|context| entry::get_member(context, options, "minLength"));
    let min_length = entry::number_of(min_length).unwrap_or(0.0).max(0.0) as usize;
    let max_length_value = entry::with_runtime(|context| entry::get_member(context, options, "maxLength"));
    let max_length = entry::number_of(max_length_value).map(|value| value.max(0.0) as usize);

    let mut open_options = std::fs::OpenOptions::new();
    open_options.create(true);
    match append {
        true => open_options.append(true),
        false => open_options.write(true).truncate(true),
    };
    let Ok(file) = open_options.open(&path_text) else {
        return entry::with_runtime(|context| {
            let prototype = chained_prototype(context, METHODS);
            self_or_new(context, this, prototype)
        });
    };
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| table.insert(id, Entry { file, buffer: String::new(), min_length, max_length }));

    entry::with_runtime(|context| {
        let prototype = chained_prototype(context, METHODS);
        let instance = self_or_new(context, this, prototype);
        let events = entry::make_object(context);
        entry::put_member(context, instance, "__events__", events);
        let id_value = entry::make_number(id as f64);
        entry::put_member(context, instance, "__streamId", id_value);
        let append_value = entry::boolean_value(append);
        entry::put_member(context, instance, "append", append_value);
        let content_mode = entry::make_string(context, "utf8");
        entry::put_member(context, instance, "contentMode", content_mode);
        instance
    })
}

/// `utf8Stream.write(text)` — buffers, dropping the whole write (never
/// partial) if `maxLength` would be exceeded; flushes once `minLength` is
/// reached. Always answers `true` — this crate's writes are synchronous, so
/// there is no backpressure state to report `false` from.
extern "C" fn write(_e: u64, this: u64, text: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(id) = id_of(this) else {
        return entry::boolean_value(false);
    };
    let Some(text) = entry::text_of(text) else {
        return entry::boolean_value(false);
    };
    let over_cap = with_table(|table| {
        table.get(&id).and_then(|entry| entry.max_length).is_some_and(|max_length| {
            table.get(&id).map(|entry| entry.buffer.len() + text.len() > max_length).unwrap_or(false)
        })
    });
    if over_cap {
        emit(this, "drop", entry::undefined_value());
        return entry::boolean_value(true);
    }
    let should_flush = with_table(|table| {
        table.get_mut(&id).map(|entry| {
            entry.buffer.push_str(&text);
            entry.buffer.len() >= entry.min_length
        })
    })
    .unwrap_or(false);
    if should_flush {
        flush_now(this, id);
    }
    entry::boolean_value(true)
}

/// Writes whatever is buffered to the real file and clears it, emitting
/// `'write'` with the byte count — a no-op (no event) when the buffer is
/// empty, matching that flushing nothing is not an I/O event.
fn flush_now(this: u64, id: u64) {
    let flushed = with_table(|table| {
        let entry = table.get_mut(&id)?;
        if entry.buffer.is_empty() {
            return None;
        }
        let bytes = std::mem::take(&mut entry.buffer);
        let length = bytes.len();
        entry.file.write_all(bytes.as_bytes()).ok()?;
        Some(length)
    });
    if let Some(length) = flushed {
        let count = entry::make_number(length as f64);
        emit(this, "write", count);
    }
}

/// `utf8Stream.flush()`/`utf8Stream.flushSync()` — one function for both:
/// every write here is already synchronous, so there is no async flush to
/// distinguish (the same collapse `fs/dir.rs`'s `read`/`readSync` documents).
extern "C" fn flush(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = id_of(this) {
        flush_now(this, id);
    }
    entry::undefined_value()
}

/// `utf8Stream.end()` — flushes any remainder, matching pino's own "end
/// flushes before closing" contract; the file itself is dropped (and closed)
/// when its table entry is removed, since nothing else here reopens it.
extern "C" fn end(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if let Some(id) = id_of(this) {
        flush_now(this, id);
    }
    entry::undefined_value()
}

/// The `Utf8Stream` constructor `node:fs` exports.
///
/// Its `.prototype` is `"Utf8Stream"` made STANDALONE — NOT chained onto
/// `"EventEmitter"` here. `lib.rs::install` calls `fs::namespace` (this
/// function's caller) before `events::namespace`, so linking onto
/// `"EventEmitter"` at THIS call would register it with an empty member
/// list and win the name permanently — the exact trap `install`'s own
/// comment names and [`super::streams`]'s doc restates for `Writable`.
/// [`construct`] does the real chaining, every time a program actually
/// builds one — always after `install` finishes, when `"EventEmitter"` is
/// the real, fully-built prototype.
pub(super) fn ctor(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "Utf8Stream", METHODS);
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    ctor
}
