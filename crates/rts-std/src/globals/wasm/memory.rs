//! `WebAssembly.Memory` — a linear memory JavaScript and the wasm side share.
//!
//! # The hard part, and the honest answer to it
//!
//! `memory.buffer` must be an `ArrayBuffer` whose bytes ARE the wasm memory: a
//! program writes a byte through a `Uint8Array` and the wasm body has to read it,
//! and the other way round. This engine's buffers own their allocation — the
//! bytes live in a `Slab<Vec<u8>>` keyed by the buffer's cell, and
//! `napi_create_external_buffer` refuses, in writing, for the same reason: "the
//! choice is refusing or copying, and copying is worse". `wasmi` owns its memory
//! inside its `Store` and takes no foreign storage either, so neither side can be
//! made to point at the other's bytes.
//!
//! What this does instead is make the two **observationally** one, by copying
//! across the only boundary where either side can look at the other's work: a
//! call into an export. Before the call, JavaScript's bytes go in; after it,
//! wasm's come out. Between calls only JavaScript touches the memory, and during
//! one only wasm does — so there is no moment at which a program can observe a
//! difference.
//!
//! **That sentence is a precondition and not a reassurance, and it has one
//! named enemy**: an import. The moment a wasm body can call a JavaScript
//! function, that function runs with the memory mid-flight and would read the
//! stale copy. `mod.rs` records that imports are absent, `store::call` holds its
//! lock across the call for the same reason, and the lot that supplies them has
//! to replace this mechanism rather than extend it.
//!
//! # The cost, stated
//!
//! Two copies of the whole linear memory per exported call — 128 KiB of
//! `memcpy` for the one-page module in the fixture, and 200 MiB for a 100 MiB
//! Emscripten heap, which is why this is a step and not the destination. The
//! destination is an `ArrayBuffer` over foreign bytes in `rts-core`, which closes
//! this and `napi_create_external_buffer` with one mechanism. Nothing here is
//! built in a way that mechanism would have to undo: the JavaScript side already
//! holds one buffer for the life of the memory, so switching to shared storage
//! deletes [`sync_in`] and [`sync_out`] and changes nothing else.
//!
//! # Why the buffer's identity is preserved
//!
//! `memory.buffer` answers the SAME `ArrayBuffer` every time, and the bytes are
//! written into it rather than a fresh one being made. A program that keeps
//! `const view = new Uint8Array(memory.buffer)` across calls — which every
//! Emscripten module does, as `HEAPU8` — would otherwise be left holding a view
//! onto a buffer nothing updates.
//!
//! A `grow` is the one case where the buffer is replaced, and that is the
//! specification's own rule rather than an artefact: it detaches the old buffer,
//! which is what `entry::detach_buffer` does for the language's `transfer`.

use super::store;
use rts_core::entry::{self, Context};
use std::sync::Mutex;

/// The property a `Memory` carries its row in.
const STAMP: &str = "__wasmMemory__";

/// Every `Memory` a program has been handed.
static MEMORIES: Mutex<Vec<Linear>> = Mutex::new(Vec::new());

/// A linear memory's two sides.
struct Linear {
    /// Which live instance's store the `wasmi::Memory` belongs to.
    instance: usize,
    memory: wasmi::Memory,
    /// The `Uint8Array` the bytes are written through — a view, because
    /// `entry::write_bytes` takes one, and it is what keeps the buffer's identity.
    view: u64,
    /// The `ArrayBuffer` `memory.buffer` answers.
    buffer: u64,
    /// The `Memory` object itself, whose `buffer` property a grow rewrites.
    ///
    /// Written after the object exists, which is why it starts as `undefined`:
    /// the row has to be in the table before `put_member` can stamp its index.
    object: u64,
    /// The ids `entry::hold_current` gave for the three values above.
    ///
    /// Without them the collector cannot see any of the three: a `u64` in this
    /// table is a reference it is not told about, which is the class
    /// `rts-core`'s rule 10 and `docs/engine/lost-roots.md` are about.
    ///
    /// **The object is held for the opposite reason to the other two, and it is
    /// the one worth stating.** A program that drops its `Memory` leaves this
    /// table still naming the cell; collected, that cell is reused, and the
    /// `put_member` a later grow performs would write `buffer` onto whatever
    /// object now lives there. Nothing would crash. That is the dangling half of
    /// the same class, and the fix is the same word.
    held: [u32; 3],
}

/// `WebAssembly.Memory` — the constructor.
pub(super) fn class(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "Memory", &[("grow", grow)]);
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::declare_host_class(context, ctor, prototype, "Memory", 1);
    ctor
}

/// `new WebAssembly.Memory({ initial, maximum? })` — a memory of its own, in a
/// store of its own.
extern "C" fn construct(_e: u64, this: u64, descriptor: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let initial = entry::with_runtime(|context| {
        let held = entry::get_member(context, descriptor, "initial");
        entry::number_of(held).unwrap_or(0.0)
    });
    let pages = initial.max(0.0) as u32;
    match store::standalone_memory(pages) {
        Ok((instance, memory)) => made(this, instance, memory),
        Err(why) => {
            entry::throw_type_error(&format!("WebAssembly.Memory(): {why}"));
            entry::undefined_value()
        }
    }
}

/// A `Memory` object over a `wasmi::Memory` that already exists.
///
/// Reached from [`construct`] and from `instance.rs`, which is why it takes the
/// pieces rather than reading them.
pub(super) fn made(this: u64, instance: usize, memory: wasmi::Memory) -> u64 {
    let bytes = store::memory_bytes(instance, memory);
    let (view, buffer) = entry::with_runtime(|context| {
        let view = entry::make_bytes(context, &bytes);
        let buffer = entry::get_member(context, view, "buffer");
        (view, buffer)
    });
    // Held BEFORE anything else can allocate: `make_prototype` and `put_member`
    // below both can, and a value this table names is invisible to the collector
    // until it is told.
    let held = [entry::hold_current(view), entry::hold_current(buffer), 0];
    let mut table = MEMORIES.lock().expect("the memory table lock");
    table.push(Linear { instance, memory, view, buffer, object: entry::undefined_value(), held });
    let row = table.len() - 1;
    drop(table);
    let object = entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Memory", &[("grow", grow)]);
        let object = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        entry::put_member(context, object, STAMP, entry::make_number(row as f64));
        entry::put_member(context, object, "buffer", buffer);
        object
    });
    let object_held = entry::hold_current(object);
    if let Some(linear) = MEMORIES.lock().expect("the memory table lock").get_mut(row) {
        linear.object = object;
        linear.held[2] = object_held;
    }
    object
}

/// `memory.grow(pages)` — the page count it had, and a new buffer.
extern "C" fn grow(_e: u64, this: u64, pages: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let Some(row) = row_of(this) else {
        entry::throw_type_error("WebAssembly.Memory.prototype.grow: not a Memory");
        return entry::undefined_value();
    };
    let delta = entry::number_for_host(pages).max(0.0) as u32;
    // JavaScript's bytes go in first: a grow keeps what was written, and the wasm
    // side is the one that grows, so anything written since the last call would
    // otherwise be dropped on the floor.
    sync_in(row);
    let (instance, memory) = match held_at(row, |linear| (linear.instance, linear.memory)) {
        Some(found) => found,
        None => return entry::undefined_value(),
    };
    let before = match store::grow_memory(instance, memory, delta) {
        Ok(before) => before,
        Err(why) => {
            // `-1` rather than a throw, which is what the specification says a
            // refused grow answers — and what a program branches on.
            let _ = why;
            return entry::make_number(-1.0);
        }
    };
    rebuild(row);
    entry::make_number(f64::from(before))
}

/// Replaces the buffer after a grow, detaching the one that was there.
///
/// The detach is the specification's: `memory.buffer` is a different object after
/// a grow and the old one has a byte length of zero. `entry::detach_buffer` is
/// what the language's own `ArrayBuffer.prototype.transfer` uses, so this is the
/// same state a program can already reach.
fn rebuild(row: usize) {
    let Some((instance, memory)) = held_at(row, |linear| (linear.instance, linear.memory)) else {
        return;
    };
    rebuild_with(row, &store::memory_bytes(instance, memory));
}

/// The same, over bytes the caller already has.
///
/// Reached from inside a call, where [`rebuild`] cannot be: the instance is TAKEN
/// out of the table while it runs (`store.rs` says why), so reading its memory
/// through the table would answer an empty slice — and the buffer would be
/// replaced with an empty one, which is a worse wrong answer than the one this
/// whole path exists to fix.
fn rebuild_with(row: usize, bytes: &[u8]) {
    let Some((old_view, old_buffer, held)) =
        held_at(row, |linear| (linear.view, linear.buffer, linear.held))
    else {
        return;
    };
    entry::detach_buffer(old_view);
    entry::detach_buffer(old_buffer);
    let (view, buffer) = entry::with_runtime(|context| {
        let view = entry::make_bytes(context, bytes);
        let buffer = entry::get_member(context, view, "buffer");
        (view, buffer)
    });
    let fresh = [entry::hold_current(view), entry::hold_current(buffer), held[2]];
    // The first two only: the object did not change, and releasing its hold here
    // would leave the row naming a cell nothing roots.
    entry::release_current(held[0]);
    entry::release_current(held[1]);
    let object = {
        let mut table = MEMORIES.lock().expect("the memory table lock");
        match table.get_mut(row) {
            Some(linear) => {
                linear.view = view;
                linear.buffer = buffer;
                linear.held = fresh;
                linear.object
            }
            None => return,
        }
    };
    // `memory.buffer` is a data property, not an accessor — `entry::define_getter`
    // takes an already-interned key a host cannot mint, which `storage.rs`,
    // `perf_hooks` and `process::info` each name for the same reason. So the
    // property is rewritten here, which is the one place a grow can do it.
    entry::with_runtime(|context| entry::put_member(context, object, "buffer", buffer));
}

/// Writes JavaScript's bytes into the wasm memory. Called before an export runs.
pub(super) fn sync_in(row: usize) {
    let Some((instance, memory, view)) =
        held_at(row, |linear| (linear.instance, linear.memory, linear.view))
    else {
        return;
    };
    let bytes = entry::with_runtime(|context| entry::bytes_of(context, view));
    if let Some(bytes) = bytes {
        store::write_memory(instance, memory, &bytes);
    }
}

/// Reads the wasm memory back into JavaScript's buffer. Called after an export
/// runs — into the SAME buffer, which is what keeps a held `Uint8Array` valid,
/// unless the memory grew, in which case [`push_to_js`] replaces it.
pub(super) fn sync_out(row: usize) {
    let Some((instance, memory)) = held_at(row, |linear| (linear.instance, linear.memory)) else {
        return;
    };
    let bytes = store::memory_bytes(instance, memory);
    // Through the same function the traversals use, rather than a `write_bytes` of
    // its own: the grow check belongs to every copy OUT, not only to the ones made
    // from inside a call. Written separately here, this path truncated silently to
    // the old length — which is the same defect in a second place.
    write_into(row, &bytes);
}

/// The one copy out: replaces the buffer when the memory has grown, then writes.
///
/// # The grow nobody asked for, and the silent wrong answer it produced
///
/// A memory grows two ways, and only one of them comes through this module: a
/// wasm body runs the `memory.grow` INSTRUCTION itself, which is what
/// `wasm-bindgen`'s allocator does on its first allocation. The buffer then still
/// has the old length, so the bytes a program writes land past its end — and a
/// write past the end of a `Uint8Array` is SILENTLY IGNORED. `md5("hello")`
/// through the `whatsapp-rust-bridge` answered `ca9c491a…`, the hash of zeros,
/// where node answers `5d41402a…`.
///
/// Nothing threw, nothing was empty, and the value looked like a hash. It was
/// found by comparing against node — which is the only thing that could have
/// found it, and the reason a fixture here asserts a KNOWN digest rather than
/// that a digest came back.
///
/// Replacing the buffer is what the specification says a grow does, and
/// `wasm-bindgen` already handles it: it re-reads its cached view when
/// `byteLength` is 0, which is what a detach leaves behind.
fn write_into(row: usize, bytes: &[u8]) {
    let Some(view) = held_at(row, |linear| linear.view) else { return };
    let room = entry::with_runtime(|context| entry::bytes_of(context, view)).map_or(0, |held| held.len());
    if room != bytes.len() {
        rebuild_with(row, bytes);
    }
    let Some(view) = held_at(row, |linear| linear.view) else { return };
    entry::with_runtime(|context| entry::write_bytes(context, view, 0, bytes));
}

/// The rows every memory of one instance occupies, for the call trampoline.
pub(super) fn rows_of_instance(instance: usize) -> Vec<usize> {
    MEMORIES
        .lock()
        .expect("the memory table lock")
        .iter()
        .enumerate()
        .filter(|(_, linear)| linear.instance == instance)
        .map(|(row, _)| row)
        .collect()
}

/// The row a `Memory` object carries.
fn row_of(value: u64) -> Option<usize> {
    let held = entry::with_runtime(|context| entry::get_member(context, value, STAMP));
    entry::number_of(held).map(|n| n as usize)
}

/// Reads from one row under the lock, so no caller holds it across a call into
/// the runtime.
fn held_at<T>(row: usize, body: impl FnOnce(&Linear) -> T) -> Option<T> {
    MEMORIES.lock().expect("the memory table lock").get(row).map(body)
}

/// Copies wasm's bytes into the JavaScript buffer of every memory of an
/// INSTANCE — the traversal `imports.rs` performs when control leaves wasm.
///
/// Keyed by the instance rather than by a memory row, because that is what a host
/// function has: `HostState` carries the instance's row and `Caller` carries the
/// memory, and the pairing is this table.
pub(super) fn push_to_js(instance: usize, bytes: &[u8]) {
    for row in rows_of_instance(instance) {
        write_into(row, bytes);
    }
}

/// The JavaScript bytes of an instance's first memory, for the traversal back.
///
/// The FIRST, because a host function reads `caller.get_export("memory")` and a
/// module with several memories is beyond what this surface accepts anyway — the
/// multi-memory proposal is not in `wasmi` 0.31 either, so a module here has at
/// most one.
pub(super) fn pull_from_js(instance: usize) -> Option<Vec<u8>> {
    let row = rows_of_instance(instance).into_iter().next()?;
    let view = held_at(row, |linear| linear.view)?;
    entry::with_runtime(|context| entry::bytes_of(context, view))
}
