//! `Blob` and `File` — the two WHATWG classes `node:buffer` hosts.
//!
//! # Why the bytes live beside the object rather than on it
//!
//! A `Blob` is immutable after construction and `blob.slice()` is a VIEW, not a
//! copy — the reference §5.1 says so and a program can observe it through
//! nothing but `size`, so the freedom is real. Both facts are lost if the bytes
//! are a property: a property is assignable, and two slices of one blob would
//! each carry their own copy of the same megabytes.
//!
//! So the bytes are an `Arc<[u8]>` in [`TABLE`], and an instance carries only
//! the number that finds them. That is the shape `string_decoder` and
//! `fs::dir` already use, reached rather than re-derived, and it inherits their
//! stated cost too: there is no `FinalizationRegistry` this crate can drive, so
//! a table entry outlives the object that named it.
//!
//! # The borrow rule, concretely
//!
//! Reading one source part needs [`entry::string_in`], [`entry::is_object`] and
//! [`entry::bytes_of`] — all context-taking — so [`part_bytes`] does the whole
//! read in ONE borrow. Walking the source ARRAY needs [`entry::get_indexed`],
//! which is ambient and would abort inside that borrow, so [`gather`] opens and
//! closes a borrow per part instead of holding one across the walk.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rts_core::entry::{self, Context, Provided};

/// One blob's bytes, plus the window this particular `Blob` is onto them.
///
/// `Arc` and not `Vec`: `slice()` mints a second entry over the same allocation,
/// which is what makes a slice a view.
struct BlobBytes {
    bytes: Arc<[u8]>,
    offset: usize,
    length: usize,
    mime: String,
}

impl BlobBytes {
    /// A copy of just this blob's window — the form every reader wants, and a
    /// copy because the caller reads it after the table lock is gone.
    fn window(&self) -> Vec<u8> {
        self.bytes
            .get(self.offset..self.offset + self.length)
            .unwrap_or(&[])
            .to_vec()
    }
}

static TABLE: Mutex<Option<HashMap<u64, BlobBytes>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_table<T>(body: impl FnOnce(&mut HashMap<u64, BlobBytes>) -> T) -> T {
    let mut guard = TABLE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let table = guard.get_or_insert_with(HashMap::new);
    body(table)
}

fn store(held: BlobBytes) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_table(|table| table.insert(id, held));
    id
}

const BLOB_METHODS: &[(&str, Provided)] = &[
    ("slice", slice_method),
    ("text", text_method),
    ("arrayBuffer", array_buffer_method),
    ("bytes", bytes_method),
    ("stream", stream_method),
];

/// The `Uint8Array` a `blob.stream()` source hands to its controller.
const CHUNK: &str = "__chunk";

/// Puts `Blob` and `File` on the `node:buffer` namespace.
pub(super) fn install(context: &mut Context, namespace: u64) {
    let (blob, file) = classes(context);
    entry::put_member(context, namespace, "Blob", blob);
    entry::put_member(context, namespace, "File", file);
}

/// The `Blob` and `File` constructors, made once per context.
///
/// # Why this is idempotent and why that had to be arranged
///
/// `Blob` is BOTH a member of `node:buffer` and an ambient global, and the two
/// must be one cell: `globalThis.Blob === (await import("node:buffer")).Blob` is
/// observable, and so is `x instanceof Blob` when a program mixes the two
/// spellings. `node:buffer` is a deferred module — it is built the first time
/// something imports it, which may be long after `lib.rs` bound the global — so
/// whichever of the two asks first has to mint the pair and the other has to
/// find it.
///
/// The pair is remembered on the PROTOTYPE, under `constructor`. That is the
/// property the language already specifies for it (`blob.constructor === Blob`,
/// which was missing until now), `make_prototype` already answers the same
/// prototype for a name, and it is the only per-CONTEXT place a host module may
/// keep a value. A `static` would be process-global where a context is
/// per-thread — the same reasoning every `Aside`-shaped table here records.
pub(crate) fn classes(context: &mut Context) -> (u64, u64) {
    let blob_prototype = blob_prototype(context);
    let blob = class_of(context, "Blob", blob_prototype, construct_blob);
    let file_prototype = file_prototype(context);
    let file = class_of(context, "File", file_prototype, construct_file);
    (blob, file)
}

/// One class: the constructor recorded on its prototype, or a fresh one.
///
/// The recorded one counts only when its own `prototype` is the one asked
/// about, and that is not a belt-and-braces check: `File.prototype` inherits
/// from `Blob.prototype`, `get_member` walks the chain, and without this the
/// second call read `Blob`'s `constructor` through the link and made `File` and
/// `Blob` the same class — `new File([...], "x.txt")` ran `construct_blob` and
/// answered an object with no `name` at all.
fn class_of(context: &mut Context, name: &str, prototype: u64, construct: Provided) -> u64 {
    let held = entry::get_member(context, prototype, "constructor");
    if held != entry::undefined_in(context)
        && entry::get_member(context, held, "prototype") == prototype
    {
        return held;
    }
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::put_member(context, prototype, "constructor", ctor);
    // `name` as a data property, because a native callable has none in this
    // engine — so `x.constructor.name`, which is how a program says what it is
    // holding, reads `undefined` for every host class here. Written for the two
    // this file owns; the engine-wide answer belongs beside `make_callable`.
    let held = entry::make_string(context, name);
    entry::put_member(context, ctor, "name", held);
    ctor
}

fn blob_prototype(context: &mut Context) -> u64 {
    entry::make_prototype(context, "Blob", BLOB_METHODS)
}

/// `File extends Blob`, expressed the only way it can be: the prototype chain.
/// `File` adds no method of its own — `name`/`lastModified` are own properties
/// of the instance — so its prototype exists purely to carry the link, which is
/// what makes `file.text()` find `Blob.prototype.text` by the ordinary walk.
fn file_prototype(context: &mut Context) -> u64 {
    let parent = blob_prototype(context);
    let prototype = entry::make_prototype(context, "File", &[]);
    entry::set_prototype_in(context, prototype, parent);
    prototype
}

/// What a `BlobOptions`/`FileOptions` argument said.
struct Settings {
    mime: String,
    native_endings: bool,
    last_modified: Option<f64>,
}

/// Reads the options object in one borrow.
///
/// `string_in`/`number_of` rather than `text_in`/`text_of`: these ask what a
/// field IS. `endings: 1` is not `endings: "native"`, and a coercion would make
/// it one.
fn settings_of(options: u64) -> Settings {
    entry::with_runtime(|context| {
        let mime = entry::get_member(context, options, "type");
        let endings = entry::get_member(context, options, "endings");
        let last_modified = entry::get_member(context, options, "lastModified");
        Settings {
            // Lowercased, per the reference §2.1 — and NOT validated, also per
            // the reference: a garbage MIME type is accepted verbatim.
            mime: entry::string_in(context, mime).unwrap_or_default().to_lowercase(),
            native_endings: entry::string_in(context, endings).as_deref() == Some("native"),
            last_modified: entry::number_of(last_modified),
        }
    })
}

/// Every source part's bytes, concatenated.
fn gather(sources: u64, native_endings: bool) -> Vec<u8> {
    let length_key = entry::with_runtime(|context| entry::make_string(context, "length"));
    let count = entry::number_of(entry::get_indexed(sources, length_key))
        .filter(|length| length.is_finite() && *length > 0.0)
        .map_or(0, |length| length as usize);
    let mut out = Vec::new();
    for index in 0..count {
        let part = entry::get_indexed(sources, entry::make_number(index as f64));
        out.extend_from_slice(&part_bytes(part, native_endings));
    }
    out
}

/// One `BlobPart`'s bytes, in one borrow.
///
/// The order is the type test, most specific first: a real string; a non-object
/// (where Node's own `BlobPart` handling is `ToString`, which is the ONE place
/// here a coercion is the specification rather than a mistaken type test); a
/// `Blob`, whose window is copied out of [`TABLE`]; and finally any view over
/// bytes. An object that is none of those contributes nothing — the
/// `ArrayBuffer` gap the module doc refuses by name.
fn part_bytes(part: u64, native_endings: bool) -> Vec<u8> {
    entry::with_runtime(|context| {
        if let Some(text) = entry::string_in(context, part) {
            return with_endings(&text, native_endings).into_bytes();
        }
        if !entry::is_object(context, part) {
            let text = entry::text_in(context, part).unwrap_or_default();
            return with_endings(&text, native_endings).into_bytes();
        }
        if let Some(id) = held_id(context, part) {
            if let Some(bytes) = with_table(|table| table.get(&id).map(BlobBytes::window)) {
                return bytes;
            }
        }
        entry::bytes_of(context, part).unwrap_or_default()
    })
}

/// `endings: 'native'` — a genuine Windows/POSIX behavioural difference the
/// reference §4 asks for, applied to STRING parts only (byte sources are
/// copied verbatim, per the same section).
///
/// Every existing line terminator is folded to `\n` first, so a source already
/// written with CRLF does not become CRCRLF on Windows.
fn with_endings(text: &str, native: bool) -> String {
    if !native || !cfg!(windows) {
        return text.to_owned();
    }
    text.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n")
}

/// The [`TABLE`] key an instance carries, if it is one of ours.
fn held_id(context: &mut Context, value: u64) -> Option<u64> {
    let held = entry::get_member(context, value, "__blobId");
    entry::number_of(held).map(|number| number as u64)
}

/// Writes the facts a program reads off a blob, and the key that finds its
/// bytes.
fn stamp(context: &mut Context, instance: u64, id: u64, length: usize, mime: &str) {
    let id_value = entry::make_number(id as f64);
    entry::put_member(context, instance, "__blobId", id_value);
    let size = entry::make_number(length as f64);
    entry::put_member(context, instance, "size", size);
    let mime = entry::make_string(context, mime);
    entry::put_member(context, instance, "type", mime);
}

/// `new Blob(sources?, options?)`.
///
/// Works called plainly too — the same `is_object`-on-`this` pattern
/// `string_decoder`/`events` use, and for the same reason: a native cannot tell
/// `new C()` from `C()` any other way.
extern "C" fn construct_blob(_e: u64, this: u64, sources: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let settings = settings_of(options);
    let bytes = gather(sources, settings.native_endings);
    let length = bytes.len();
    let id = store(BlobBytes {
        bytes: Arc::from(bytes),
        offset: 0,
        length,
        mime: settings.mime.clone(),
    });
    entry::with_runtime(|context| {
        let prototype = blob_prototype(context);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        stamp(context, instance, id, length, &settings.mime);
        instance
    })
}

/// `new File(sources, fileName, options?)`.
extern "C" fn construct_file(_e: u64, this: u64, sources: u64, name: u64, options: u64, _d: u64) -> u64 {
    let settings = settings_of(options);
    let bytes = gather(sources, settings.native_endings);
    let length = bytes.len();
    let id = store(BlobBytes {
        bytes: Arc::from(bytes),
        offset: 0,
        length,
        mime: settings.mime.clone(),
    });
    let last_modified = settings.last_modified.unwrap_or_else(now_ms);
    entry::with_runtime(|context| {
        let prototype = file_prototype(context);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        stamp(context, instance, id, length, &settings.mime);
        // `text_in`, not `string_in`: Node's `fileName` is `String(fileName)`,
        // so coercing is the specified behaviour here rather than a type test.
        let file_name = entry::text_in(context, name).unwrap_or_default();
        let file_name = entry::make_string(context, &file_name);
        entry::put_member(context, instance, "name", file_name);
        let stamped = entry::make_number(last_modified);
        entry::put_member(context, instance, "lastModified", stamped);
        instance
    })
}

/// `Date.now()` from Rust, for a `File` given no `lastModified`.
fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |since| since.as_millis() as f64)
}

/// The window and MIME type of the receiver, if it is one of ours.
fn window_of(this: u64) -> Option<(Vec<u8>, String)> {
    let id = entry::with_runtime(|context| held_id(context, this))?;
    with_table(|table| {
        table.get(&id).map(|held| (held.window(), held.mime.clone()))
    })
}

/// `blob.slice(start?, end?, type?)` — a VIEW over the same `Arc`, never a
/// copy, which is what the reference §5.1 specifies.
extern "C" fn slice_method(_e: u64, this: u64, start: u64, end: u64, kind: u64, _d: u64) -> u64 {
    let Some(id) = entry::with_runtime(|context| held_id(context, this)) else {
        return entry::undefined_value();
    };
    let mime = entry::with_runtime(|context| entry::string_in(context, kind)).unwrap_or_default();
    let bounds = (entry::number_of(start), entry::number_of(end));
    let made = with_table(|table| {
        let held = table.get(&id)?;
        let size = held.length as f64;
        let from = clamp(bounds.0.unwrap_or(0.0), size);
        let to = clamp(bounds.1.unwrap_or(size), size);
        let length = to.saturating_sub(from);
        Some(BlobBytes {
            bytes: Arc::clone(&held.bytes),
            offset: held.offset + from,
            length,
            mime: mime.to_lowercase(),
        })
    });
    let Some(made) = made else {
        return entry::undefined_value();
    };
    let length = made.length;
    let mime = made.mime.clone();
    let id = store(made);
    entry::with_runtime(|context| {
        let prototype = blob_prototype(context);
        let instance = entry::make_instance(context, prototype);
        stamp(context, instance, id, length, &mime);
        instance
    })
}

/// A `slice` bound: negative counts from the end, out of range clamps — the
/// WHATWG rule, and the reason it is arithmetic here rather than a cast is that
/// `as usize` on a negative double is silently zero.
fn clamp(bound: f64, size: f64) -> usize {
    let resolved = if bound < 0.0 { size + bound } else { bound };
    resolved.clamp(0.0, size) as usize
}

/// `blob.text()` — a promise of the UTF-8 decoding.
///
/// Already settled, because the bytes are resident: there is no asynchrony to
/// express, only the promise SHAPE Node's signature has. `entry::settled`
/// documents that trade once; nothing is re-decided here.
extern "C" fn text_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = window_of(this).map(|(bytes, _)| entry::decode_bytes(&bytes, "utf8"));
    entry::with_runtime(|context| {
        let value = match text {
            Some(text) => entry::make_string(context, &text),
            None => entry::undefined_in(context),
        };
        entry::settled(context, value, false)
    })
}

/// `blob.bytes()` — a settled promise of a `Uint8Array`, which is what the
/// reference says this one answers (`arrayBuffer` is the `ArrayBuffer` form).
extern "C" fn bytes_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held = window_of(this).map(|(bytes, _)| bytes);
    entry::with_runtime(|context| {
        let value = match held {
            Some(bytes) => entry::make_bytes(context, &bytes),
            None => entry::undefined_in(context),
        };
        entry::settled(context, value, false)
    })
}

/// `blob.stream()` — a `ReadableStream` over the blob's bytes.
///
/// # Why the class comes from the global object
///
/// Because it must be the SAME `ReadableStream` a program gets from
/// `new ReadableStream()`, or `blob.stream() instanceof ReadableStream` is
/// false for the object this method just made. That class is `rts-std`'s and
/// this crate does not depend on it, so the global is the only route across —
/// the mirror of the one `globals/fetch/` takes to reach `Blob` itself.
/// `undefined` when nothing installed the name, which lets a host without
/// streams degrade instead of crash.
///
/// # Why one chunk
///
/// The bytes are resident: they were gathered at construction. Splitting them
/// would invent a boundary no program asked for, and the reference gives a
/// blob's stream no chunk size. An EMPTY blob enqueues nothing at all — a
/// zero-length chunk is a value a `for await` would see, and Bun and Node both
/// answer no chunks there.
extern "C" fn stream_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let class = global("ReadableStream");
    if !entry::with_runtime(|context| entry::is_callable_in(context, class)) {
        return absent;
    }
    let bytes = window_of(this).map_or_else(Vec::new, |(bytes, _)| bytes);
    let source = entry::with_runtime(|context| {
        let source = entry::make_object(context);
        if !bytes.is_empty() {
            let chunk = entry::make_bytes(context, &bytes);
            entry::put_member(context, source, CHUNK, chunk);
        }
        let start = entry::make_callable(context, stream_start);
        entry::put_member(context, source, "start", start);
        source
    });
    entry::construct(class, source, absent, absent, absent)
}

/// The `start(controller)` of the source [`stream_method`] builds — `this` is
/// that source, which is where the chunk was left for it.
///
/// The controller's own methods are called rather than any internal reached
/// for: this crate can see nothing of how a `ReadableStream` holds its queue,
/// and the two `thrown()` questions are rule 8 of `rts-core`'s README — a
/// controller whose `enqueue` raised must not then be told to `close`.
extern "C" fn stream_start(_e: u64, this: u64, controller: u64, _b: u64, _c: u64, _d: u64) -> u64 {
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

/// `blob.arrayBuffer()` — a settled promise of an `ArrayBuffer`.
///
/// Reached through the `buffer` property of a fresh `Uint8Array` rather than
/// built here: `rts-core` stamps that property with the real backing cell,
/// and it is sized to exactly these bytes because [`entry::make_bytes`]
/// allocates per call. There is no host entry that mints a bare `ArrayBuffer`,
/// and inventing one here would be a second answer to what an `ArrayBuffer` is.
extern "C" fn array_buffer_method(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let held = window_of(this).map(|(bytes, _)| bytes);
    entry::with_runtime(|context| {
        let value = match held {
            Some(bytes) => {
                let view = entry::make_bytes(context, &bytes);
                entry::get_member(context, view, "buffer")
            }
            None => entry::undefined_in(context),
        };
        entry::settled(context, value, false)
    })
}
