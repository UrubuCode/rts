//! The nine streaming classes and their `createX()` factories.
//!
//! # Why these exist now, when this module refused them
//!
//! The previous refusal said the mechanism was unavailable: an incremental
//! codec is state living longer than one native call, and this crate has "no
//! handle table of its own to hold it in". That was wrong on the second half.
//! `crypto/hash.rs` holds a mid-update `sha2::Sha256` in exactly such a table
//! — a `Mutex<HashMap<u64, _>>` keyed by a hidden number on the instance —
//! and states in its own module doc that `fs/fd.rs` and `fs/dir.rs` key an
//! open file and a directory cursor the same way. So the pattern is
//! established infrastructure, not a second table invented for zlib, and this
//! file follows it rather than proposing anything.
//!
//! # How a native subclasses `Transform` with no `class` keyword
//!
//! Three steps, and the borrow discipline is what dictates the shape:
//!
//! 1. Inside one borrow: read `node:stream`'s `Transform` and its
//!    `prototype`, make this class's own prototype, link the two, and make
//!    the instance.
//! 2. OUTSIDE the borrow: call the `Transform` constructor with `this` set to
//!    that instance. `entry::call` is ambient — it takes its own borrow — so
//!    doing this inside step one aborts the process. `stream/duplex.rs`'s
//!    `self_or_new` keeps the prototype we already set, which is what makes
//!    the instance come back a `Gzip` and not a bare `Transform`.
//! 3. Inside a second borrow: install this file's `_transform`/`_flush` as
//!    OWN properties. `build_transform` only reads those from an options
//!    object, and mutating the CALLER's options object to smuggle them in
//!    would be a visible side effect on a value the program still holds.
//!
//! The parent prototype is read off the constructor rather than asked for by
//! name through `make_prototype(context, "Transform", &[])`: that call is
//! idempotent by name and REGISTERS on a miss, so if it ever ran before
//! `node:stream` built its own it would win the name with an empty member
//! list and silently strip every method off every Transform in the program.
//! `lib.rs`'s own comment records that exact accident happening to
//! `EventEmitter`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rts_core::entry::{self, Context, Provided};

use super::codec::{Kind, Settings, Sink};
use super::options;

/// A live stream's codec state. `sink` is an `Option` because `close()`
/// takes it: a closed stream that kept a usable sink would go on compressing
/// after the program said it was done.
struct Live {
    kind: Kind,
    settings: Settings,
    sink: Option<Sink>,
    written: u64,
}

static TABLE: Mutex<Option<HashMap<u64, Live>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, Live>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

/// The id linking an instance to its row in [`TABLE`].
///
/// Read through `get_member`, which takes the context, and never through
/// `get_indexed`, which is ambient: every caller here is already inside
/// `with_runtime`, where the ambient form is a nested borrow and therefore an
/// abort rather than an error.
fn stream_id(context: &mut Context, this: u64) -> Option<u64> {
    let value = entry::get_member(context, this, "__zlibId");
    entry::number_of(value).map(|number| number as u64)
}

const METHODS: &[(&str, Provided)] = &[("flush", flush), ("reset", reset), ("close", close)];

/// This class's prototype, chained onto the real `Transform.prototype`.
///
/// Answers `None` when `node:stream` is not registered — which is a refusal
/// the caller turns into `undefined`, not a bare object pretending to be a
/// stream.
fn class_prototype(context: &mut Context, kind: Kind) -> Option<(u64, u64)> {
    let absent = entry::undefined_in(context);
    let stream = entry::module_at_name(context, "node:stream");
    if stream == absent {
        return None;
    }
    let constructor = entry::get_member(context, stream, "Transform");
    let parent = entry::get_member(context, constructor, "prototype");
    if constructor == absent || parent == absent {
        return None;
    }
    let prototype = entry::make_prototype(context, kind.class_name(), METHODS);
    entry::set_prototype_in(context, prototype, parent);
    Some((constructor, prototype))
}

/// `zlib.createGzip(options)` and `new zlib.Gzip(options)` — one function,
/// because the only difference is whether `this` is already an object, which
/// is the question `stream/common.rs`'s `self_or_new` asks for the same
/// reason.
fn create(kind: Kind, this: u64, options: u64) -> u64 {
    // FIRST, and outside everything else: `createGzip({ chunkSize: 0 })` is a
    // throw in Node (`test-zlib-failed-init.js`), and a stream built before
    // the check would already have a row in [`TABLE`] and a `Transform` parent
    // constructed — state to unwind for an object no program will receive.
    // Raised out here rather than inside the borrow that found it, because
    // raising takes a borrow of its own.
    let checked = entry::with_runtime(|context| options::settings(context, kind, options));
    let settings = match checked {
        Ok(settings) => settings,
        Err(refusal) => {
            refusal.raise();
            return entry::undefined_value();
        }
    };
    let prepared = entry::with_runtime(|context| {
        let (constructor, prototype) = class_prototype(context, kind)?;
        let instance = match entry::is_object(context, this) {
            true => {
                entry::set_prototype_in(context, this, prototype);
                this
            }
            false => entry::make_instance(context, prototype),
        };
        Some((constructor, instance))
    });
    let Some((constructor, instance)) = prepared else {
        return entry::undefined_value();
    };
    let absent = entry::undefined_value();
    // Step 2 of the module doc: ambient, so outside every borrow.
    entry::call(constructor, instance, options, absent, absent, absent);
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    entry::with_runtime(|context| {
        // The settings checked above, reused rather than read again: two reads
        // of one options object are two chances to disagree about it, and the
        // program may have mutated it inside the `Transform` constructor call
        // that runs between them.
        let sink = Sink::new(kind, &settings);
        with_table(|table| {
            table.insert(id, Live { kind, settings, sink: Some(sink), written: 0 });
        });
        let held = entry::make_number(id as f64);
        entry::put_member(context, instance, "__zlibId", held);
        let zero = entry::make_number(0.0);
        entry::put_member(context, instance, "bytesWritten", zero);
        let step = entry::make_callable(context, transform_step);
        entry::put_member(context, instance, "_transform", step);
        let ending = entry::make_callable(context, flush_step);
        entry::put_member(context, instance, "_flush", ending);
    });
    instance
}

/// `this._transform(chunk, encoding, callback)`.
///
/// The output goes back through the callback's second argument rather than
/// through an explicit `this.push(...)`: `stream/duplex.rs`'s `transform_done`
/// already pushes whatever `_transform` hands back, and pushing here as well
/// would emit every chunk twice.
extern "C" fn transform_step(_e: u64, this: u64, chunk: u64, _encoding: u64, callback: u64, _d: u64) -> u64 {
    let (failed, data) = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let Some(id) = stream_id(context, this) else {
            return (true, absent);
        };
        let Some(bytes) = options::input_bytes(context, chunk) else {
            return (true, absent);
        };
        let outcome = with_table(|table| {
            let live = table.get_mut(&id)?;
            let sink = live.sink.as_mut()?;
            let accepted = sink.feed(&bytes).is_ok();
            let produced = sink.drain();
            live.written += bytes.len() as u64;
            Some((accepted, produced, live.written))
        });
        let Some((accepted, produced, written)) = outcome else {
            return (true, absent);
        };
        let count = entry::make_number(written as f64);
        entry::put_member(context, this, "bytesWritten", count);
        if !accepted {
            return (true, absent);
        }
        // An empty result is not a chunk: `_transform` handing back an empty
        // `Buffer` would fire a `'data'` event carrying nothing, which is a
        // real observable difference from Node, where a codec that produced
        // no output this round simply produces no event.
        match produced.is_empty() {
            true => (false, absent),
            false => (false, entry::make_buffer(context, &produced)),
        }
    });
    let absent = entry::undefined_value();
    match failed {
        true => {
            let reason = entry::with_runtime(|context| {
                entry::make_string(context, "zlib: chunk could not be processed")
            });
            entry::call(callback, absent, reason, absent, absent, absent);
        }
        false => {
            entry::call(callback, absent, absent, data, absent, absent);
        }
    }
    absent
}

/// `this._flush(callback)` — the terminal step, run once when the writable
/// side finishes. This is where a compressor writes its trailer and where a
/// decompressor discovers its input ended early.
extern "C" fn flush_step(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let (failed, data) = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let Some(id) = stream_id(context, this) else {
            return (true, absent);
        };
        let taken = with_table(|table| table.get_mut(&id).and_then(|live| live.sink.take()));
        let Some(sink) = taken else {
            return (true, absent);
        };
        match sink.finish() {
            None => (true, absent),
            Some(bytes) if bytes.is_empty() => (false, absent),
            Some(bytes) => (false, entry::make_buffer(context, &bytes)),
        }
    });
    let absent = entry::undefined_value();
    match failed {
        true => {
            let reason = entry::with_runtime(|context| {
                entry::make_string(context, "zlib: stream ended before the codec did")
            });
            entry::call(callback, absent, reason, absent, absent, absent);
        }
        false => {
            entry::call(callback, absent, absent, data, absent, absent);
        }
    }
    absent
}

/// `zlibBase.flush([kind, ]callback?)`.
///
/// The `kind` argument is READ and discarded — see `mod.rs`'s refusal list.
/// It is still shifted out of the way rather than being mistaken for the
/// callback, which is the overload defect `fs.watch` shipped.
extern "C" fn flush(_e: u64, this: u64, a0: u64, a1: u64, _c: u64, _d: u64) -> u64 {
    let (_kind, callback) = options::options_and_callback(a0, a1);
    let data = entry::with_runtime(|context| {
        let absent = entry::undefined_in(context);
        let Some(id) = stream_id(context, this) else {
            return absent;
        };
        let produced = with_table(|table| {
            table
                .get_mut(&id)
                .and_then(|live| live.sink.as_mut())
                .map(Sink::flush_now)
                .unwrap_or_default()
        });
        match produced.is_empty() {
            true => absent,
            false => entry::make_buffer(context, &produced),
        }
    });
    let absent = entry::undefined_value();
    if data != absent {
        // Ambient, and therefore outside the borrow that produced `data`.
        let push = entry::with_runtime(|context| entry::get_member(context, this, "push"));
        entry::call(push, this, data, absent, absent, absent);
    }
    if callback != absent {
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    absent
}

/// `zlibBase.reset()` — a fresh codec of the same kind and settings.
///
/// Node restricts this to the zlib-based Inflate/Deflate classes. Here it
/// works for every kind, because "discard the codec and build another" is
/// meaningful for all of them; that is a superset of Node's surface rather
/// than a divergence a program can be wrong about.
extern "C" fn reset(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        if let Some(id) = stream_id(context, this) {
            with_table(|table| {
                if let Some(live) = table.get_mut(&id) {
                    live.sink = Some(Sink::new(live.kind, &live.settings));
                    live.written = 0;
                }
            });
            let zero = entry::make_number(0.0);
            entry::put_member(context, this, "bytesWritten", zero);
        }
        entry::undefined_in(context)
    })
}

/// `zlibBase.close(callback?)` — releases the codec. Safe to call twice, as
/// Node documents: the second call finds nothing to remove.
extern "C" fn close(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        if let Some(id) = stream_id(context, this) {
            with_table(|table| table.remove(&id));
        }
    });
    let absent = entry::undefined_value();
    if callback != absent {
        entry::call(callback, absent, absent, absent, absent, absent);
    }
    absent
}

macro_rules! stream_class {
    ($name:ident, $kind:expr) => {
        extern "C" fn $name(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
            create($kind, this, options)
        }
    };
}

stream_class!(gzip, Kind::Gzip);
stream_class!(gunzip, Kind::Gunzip);
stream_class!(deflate, Kind::Deflate);
stream_class!(inflate, Kind::Inflate);
stream_class!(deflate_raw, Kind::DeflateRaw);
stream_class!(inflate_raw, Kind::InflateRaw);
stream_class!(unzip, Kind::Unzip);
stream_class!(brotli_compress, Kind::BrotliCompress);
stream_class!(brotli_decompress, Kind::BrotliDecompress);

/// Every class, under both names a program reaches it by: `zlib.Gzip` (with
/// a real `prototype`, so `x instanceof zlib.Gzip` and `zlib.Gzip.prototype`
/// both answer) and `zlib.createGzip`.
///
/// ONE function behind both, rather than a factory that constructs the class
/// — two entry points that must agree about what a `Gzip` is are two places
/// for them to stop agreeing.
pub(super) fn install(context: &mut Context, namespace: u64) {
    let classes: &[(&str, &str, Kind, Provided)] = &[
        ("Gzip", "createGzip", Kind::Gzip, gzip),
        ("Gunzip", "createGunzip", Kind::Gunzip, gunzip),
        ("Deflate", "createDeflate", Kind::Deflate, deflate),
        ("Inflate", "createInflate", Kind::Inflate, inflate),
        ("DeflateRaw", "createDeflateRaw", Kind::DeflateRaw, deflate_raw),
        ("InflateRaw", "createInflateRaw", Kind::InflateRaw, inflate_raw),
        ("Unzip", "createUnzip", Kind::Unzip, unzip),
        ("BrotliCompress", "createBrotliCompress", Kind::BrotliCompress, brotli_compress),
        ("BrotliDecompress", "createBrotliDecompress", Kind::BrotliDecompress, brotli_decompress),
    ];
    for (class, factory, kind, code) in classes {
        let constructor = entry::make_callable(context, *code);
        // Built here so `zlib.Gzip.prototype` is the same object every
        // instance links to. `class_prototype` needs `node:stream`, which
        // `lib.rs` builds before this module — and if it somehow did not,
        // this leaves the constructor without a `prototype` member rather
        // than registering an empty one under the `Transform` name.
        if let Some((_, prototype)) = class_prototype(context, *kind) {
            entry::put_member(context, constructor, "prototype", prototype);
            // Back-link — see `crate::stream::class_ctor`'s doc.
            entry::declare_host_class(context, constructor, prototype, class, 1);
        }
        entry::put_member(context, namespace, class, constructor);
        entry::put_member(context, namespace, factory, constructor);
    }
}
