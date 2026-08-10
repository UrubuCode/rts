//! `Storage` (the Web Storage API), persisted as a byte-exact record.
//!
//! # Reuse-check: what was searched, and why nothing here reuses it
//!
//! `crates/rts-std/src/globals/storage/mod.rs` is the OLD engine's `Storage`,
//! read in full. Its shape — `getItem`/`setItem`/`removeItem`/`clear`/`key`,
//! string-only, `persistTo(path)` reloading on link and rewriting on every
//! mutation — is right and is what this module copies. Its **code** is not
//! reachable: it is written against `rts_engine::heap::pickle` and
//! `rts_engine::heap::handles::{alloc_entry, Entry}`, which are the OLD value
//! encoding `rts_cranelift::tags` replaced, and `rts-std-rwk` does not and must
//! not depend on `rts-engine`.
//!
//! `rts-core-rwk` — the crate this one DOES depend on — has no pickle format at
//! all; `entry::deep_copy` (`clone.rs`) copies a *live* value graph inside one
//! run, not a byte stream that survives a process exit, so it answers a
//! different question. Nothing in this crate's own reach answers "bytes that
//! round-trip a `Vec<(String, String)>`", so [`encode`]/[`decode`] below are
//! new, small, and deliberately not called "pickle" — that name is the old
//! engine's format, and reusing the word for a different one is exactly the
//! kind of two-answers-one-question this workspace's `CLAUDE.md` warns about.
//!
//! # The format: length-prefixed pairs, not text
//!
//! `setItem("bruto", "linha1\nlinha2=x\ty|z\r\nfim")` is the case a "key=value
//! per line" format loses: the value contains the line separator a text format
//! would use to find the *next* pair. So each string here is a **byte count**
//! followed by exactly that many bytes — never a delimiter a value could also
//! contain — which is what makes [`decode`] able to read back
//! `\n`/`\t`/`\r`/`|` inside a value byte-for-byte, checked by
//! `tests/claude-storage-pickle.test.ts`'s "valor HOSTIL" case.
//!
//! # Why availability, not `rts-core-rwk`, is why this lives here
//!
//! `persistTo` opens a file. `rts-core-rwk`'s rule 1 is availability — "present
//! on every target, including wasm" — and a filesystem is not one of those, so
//! this belongs in the host-furniture crate the way `console` and the WHATWG
//! event globals already do, per this module's own doc.

use rts_core_rwk::entry::{self, Context, Provided};
use std::sync::Mutex;

/// One `Storage`'s pairs, in insertion order (the spec exposes `key(n)` by
/// index), and the file it is linked to.
#[derive(Default, Clone)]
struct Backing {
    keys: Vec<String>,
    vals: Vec<String>,
    path: Option<String>,
}

/// Every live `Storage`, keyed by an id stamped on the instance.
///
/// # Why a side table and not an `Aside<T>` in `rts-core-rwk`
///
/// `Aside<T>` is that crate's pattern for state beside a cell it owns, and this
/// crate has no cell table of its own to key one by — `rts-core-rwk::entry`
/// hands back opaque `u64` values, never a region index this crate could index
/// into safely across a GC move. A `Mutex<Vec<Backing>>` addressed by a small
/// integer STAMPED as an own property (`__storageId__`) is the analogous shape
/// built from outside that crate, the same trade `events/target.rs`'s
/// `__listeners__` and this file's sibling `emitter.rs` both make: state that
/// would ideally live beside the cell instead lives in an ordinary property
/// plus a table here.
static BACKINGS: Mutex<Vec<Backing>> = Mutex::new(Vec::new());

const METHODS: &[(&str, Provided)] = &[
    ("getItem", get_item),
    ("setItem", set_item),
    ("removeItem", remove_item),
    ("clear", clear),
    ("key", key),
    ("persistTo", persist_to),
];

/// Installs `Storage` as a global.
///
/// # Why `length` is a stamped data property and not an accessor
///
/// `entry::define_getter` takes an already-interned property key — a key
/// number the COMPILER minted, per its own doc comment — and a host outside
/// `rts-core-rwk` has no way to mint one; `perf_hooks`, `diagnostics_channel`
/// and `process::info` each name the identical gap for the identical reason.
/// So `length` is written as an ordinary data property after every mutation,
/// the same divergence `events/target.rs`'s stamped state already accepts:
/// `storage.length = 99` would stick here where the specification refuses it.
pub fn install(context: &mut Context) {
    let prototype = entry::make_prototype(context, "Storage", METHODS);
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::declare_global(context, "Storage", ctor);
}

/// `new Storage()` — empty, in-memory, until `persistTo` links a file.
extern "C" fn construct(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Storage", METHODS);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let id = allocate();
        entry::put_member(context, instance, "__storageId__", entry::make_number(id as f64));
        entry::put_member(context, instance, "length", entry::make_number(0.0));
        instance
    })
}

/// Restamps `length` from the backing table — called after every mutation.
/// See the module doc for why this is a data property rather than a getter.
fn restamp_length(this: u64) {
    let count = with_backing(this, |backing| backing.keys.len()).unwrap_or(0);
    entry::with_runtime(|context| {
        entry::put_member(context, this, "length", entry::make_number(count as f64));
    });
}

/// `storage.getItem(key)` — the value, or `null` if the key is absent.
extern "C" fn get_item(_e: u64, this: u64, key_arg: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(key) = entry::text_of(key_arg) else { return null() };
    match with_backing(this, |backing| index_of(backing, &key).map(|i| backing.vals[i].clone())) {
        Some(Some(value)) => string(&value),
        _ => null(),
    }
}

/// `storage.setItem(key, value)` — inserts or overwrites, then persists.
extern "C" fn set_item(_e: u64, this: u64, key_arg: u64, value_arg: u64, _c: u64, _d: u64) -> u64 {
    let (Some(key), Some(value)) = (entry::text_of(key_arg), entry::text_of(value_arg)) else {
        return absent();
    };
    with_backing_mut(this, |backing| {
        match index_of(backing, &key) {
            Some(i) => backing.vals[i] = value,
            None => {
                backing.keys.push(key);
                backing.vals.push(value);
            }
        }
    });
    flush(this);
    restamp_length(this);
    absent()
}

/// `storage.removeItem(key)` — a no-op if absent, then persists.
extern "C" fn remove_item(_e: u64, this: u64, key_arg: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(key) = entry::text_of(key_arg) else { return absent() };
    let removed = with_backing_mut(this, |backing| {
        if let Some(i) = index_of(backing, &key) {
            backing.keys.remove(i);
            backing.vals.remove(i);
            true
        } else {
            false
        }
    });
    if removed == Some(true) {
        flush(this);
        restamp_length(this);
    }
    absent()
}

/// `storage.clear()`.
extern "C" fn clear(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    with_backing_mut(this, |backing| {
        backing.keys.clear();
        backing.vals.clear();
    });
    flush(this);
    restamp_length(this);
    absent()
}

/// `storage.key(n)` — the n-th key in insertion order, or `null`.
extern "C" fn key(_e: u64, this: u64, n_arg: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(n) = entry::number_of(n_arg) else { return null() };
    if n < 0.0 {
        return null();
    }
    match with_backing(this, |backing| backing.keys.get(n as usize).cloned()) {
        Some(Some(found)) => string(&found),
        _ => null(),
    }
}

/// `storage.persistTo(path)` — links this storage to a FILE and loads what is
/// already there. Host surface, not the page's: real `localStorage` never
/// takes one, since a site does not choose where its storage lives.
extern "C" fn persist_to(_e: u64, this: u64, path_arg: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(path) = entry::text_of(path_arg) else { return absent() };
    with_backing_mut(this, |backing| backing.path = Some(path.clone()));
    load(this, &path);
    restamp_length(this);
    absent()
}

/// Reloads from the linked file, replacing in-memory content. A missing,
/// unreadable, or corrupt file leaves the storage as it is — cache data is
/// never worth crashing boot over, which `tests/claude-storage-pickle.test.ts`
/// pins directly (a garbage file, then a normal `setItem`/`getItem`).
fn load(this: u64, path: &str) {
    let Ok(raw) = std::fs::read(path) else { return };
    let Some((keys, vals)) = decode(&raw) else { return };
    with_backing_mut(this, |backing| {
        backing.keys = keys;
        backing.vals = vals;
    });
}

/// Rewrites the linked file, if there is one.
fn flush(this: u64) {
    let Some((path, keys, vals)) = with_backing(this, |backing| {
        backing.path.clone().map(|path| (path, backing.keys.clone(), backing.vals.clone()))
    }).flatten() else {
        return;
    };
    let _ = std::fs::write(&path, encode(&keys, &vals));
}

/// The bytes for `keys`/`vals`: a magic tag, a pair count, then each string as
/// a little-endian `u32` byte length followed by its raw bytes — see the
/// module doc for why length-prefixing rather than a text delimiter.
fn encode(keys: &[String], vals: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(keys.len() as u32).to_le_bytes());
    for (k, v) in keys.iter().zip(vals.iter()) {
        push_string(&mut out, k);
        push_string(&mut out, v);
    }
    out
}

/// The inverse of [`encode`]. `None` for anything that is not exactly this
/// format — including a file some OTHER program wrote — which is what makes a
/// corrupt file answer "empty storage" rather than a panic.
fn decode(bytes: &[u8]) -> Option<(Vec<String>, Vec<String>)> {
    if bytes.len() < MAGIC.len() + 4 || &bytes[..MAGIC.len()] != MAGIC {
        return None;
    }
    let mut at = MAGIC.len();
    let count = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
    at += 4;
    let mut keys = Vec::with_capacity(count);
    let mut vals = Vec::with_capacity(count);
    for _ in 0..count {
        let (k, next) = pull_string(bytes, at)?;
        at = next;
        let (v, next) = pull_string(bytes, at)?;
        at = next;
        keys.push(k);
        vals.push(v);
    }
    Some((keys, vals))
}

/// This engine's own tag, so a file another program wrote is refused rather
/// than misread as zero pairs.
const MAGIC: &[u8; 8] = b"RTSSTOR1";

fn push_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text.as_bytes());
}

fn pull_string(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let len = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
    let start = at + 4;
    let text = std::str::from_utf8(bytes.get(start..start + len)?).ok()?.to_string();
    Some((text, start + len))
}

/// A fresh, empty backing, answering its id.
fn allocate() -> usize {
    let mut table = BACKINGS.lock().unwrap();
    table.push(Backing::default());
    table.len() - 1
}

/// The id an instance was stamped with, read outside any `rts-core-rwk` borrow.
fn id_of(this: u64) -> Option<usize> {
    entry::number_of(entry::get_indexed(this, string("__storageId__"))).map(|n| n as usize)
}

/// Reads through the backing table.
fn with_backing<T>(this: u64, body: impl FnOnce(&Backing) -> T) -> Option<T> {
    let id = id_of(this)?;
    let table = BACKINGS.lock().unwrap();
    table.get(id).map(body)
}

/// Writes through the backing table.
fn with_backing_mut<T>(this: u64, body: impl FnOnce(&mut Backing) -> T) -> Option<T> {
    let id = id_of(this)?;
    let mut table = BACKINGS.lock().unwrap();
    table.get_mut(id).map(body)
}

fn index_of(backing: &Backing, key: &str) -> Option<usize> {
    backing.keys.iter().position(|k| k == key)
}

fn absent() -> u64 {
    entry::undefined_value()
}

fn null() -> u64 {
    entry::null_value()
}

fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}
