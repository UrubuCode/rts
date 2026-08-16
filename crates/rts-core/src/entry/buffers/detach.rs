//! Detaching an `ArrayBuffer` from its bytes.
//!
//! # Why this is a mark and not just an empty store
//!
//! Detaching frees the bytes, and freeing them alone would be
//! indistinguishable from `new ArrayBuffer(0)` — both answer a zero
//! `byteLength` and both have nothing to read. The ABI asks the difference
//! (`napi_is_detached_arraybuffer`), and so does the language: a detached
//! buffer is one every operation refuses, while an empty one is merely empty.
//!
//! So the store is dropped AND the cell is marked, and the mark is what the
//! question is answered from.
//!
//! # What this does not yet do
//!
//! Refuse. A view over a detached buffer answers `undefined` for every index
//! here, because its window is gone; the language throws a `TypeError` on that
//! read instead. That is the same divergence every out-of-range read in
//! `buffers/mod.rs` already carries, and it is stated rather than hidden: an
//! addon detaching a buffer gets the memory back, which is what it detached
//! for, and a program reading afterwards gets a wrong answer rather than a
//! throw.
//!
//! # Why the caller is `rts-napi` and the code is here
//!
//! `detach` is a fact about the buffer representation — which slab holds the
//! bytes, which `Aside` names the store — and rule 5 of `rts-core`'s README
//! keeps that inside this crate. The ABI crate asks; it does not reach in.

use super::super::{Context, with_current};
use crate::value::Value;

impl Context {
    /// Drops a cell's byte store and marks it detached.
    ///
    /// Answers whether there was one: a value that is not an `ArrayBuffer` is
    /// the caller's `napi_arraybuffer_expected`, and detaching twice is not an
    /// error but is not a second success either.
    pub(in crate::entry) fn detach_buffer(&mut self, cell: u32) -> bool {
        // The cell keeps POINTING at its store, which looks like the opposite
        // of detaching and is not: a view resolves its window through this
        // mapping, and removing it makes every view of a detached buffer
        // answer "not a view at all" rather than "a view of nothing". The
        // bytes are what goes away; the empty store is what the views then
        // see, which is the zero length the language says they have.
        let Some(store) = self.buffer_of.copied(cell) else {
            return false;
        };
        if self.detached.copied(cell).unwrap_or(false) {
            // Already gone. Answering true twice would let an addon believe it
            // freed two buffers.
            return false;
        }
        // The slab entry itself is emptied rather than left: the point of
        // detaching is that the bytes go away, and a `Vec` still holding a
        // megabyte behind a slot nothing names is the leak this exists to
        // prevent.
        if let Ok(bytes) = self.buffers.at_mut(store) {
            bytes.clear();
            bytes.shrink_to_fit();
        }
        self.detached.set(cell, true);
        super::stamp(self, cell, "byteLength", 0.0);
        super::flag(self, cell, "detached", true);
        true
    }

    /// Whether a cell was detached.
    pub(in crate::entry) fn buffer_detached(&self, cell: u32) -> bool {
        self.detached.copied(cell).unwrap_or(false)
    }
}

/// The cell holding the bytes a value names: itself, or the buffer it views.
///
/// A view is followed because the ABI's callers hold one — `napi_create_buffer`
/// answers a `Uint8Array` here, as `buffers.rs` records — and detaching "the
/// arraybuffer" of a typed array is what an addon means either way.
fn store_cell(context: &Context, value: u64) -> Option<u32> {
    let cell = Value(value).as_slot()?;
    match super::view_of(context, value) {
        Some(view) => Some(view.buffer),
        None => Some(cell),
    }
}

/// Detaches the buffer a value names, and answers whether it was one.
///
/// The VIEW that named it is emptied too, when the value was one. Without
/// that, a `Uint8Array` over the freed store keeps saying it has eight bytes
/// and every read of it misses its own window — so `byteLength` and the view
/// would disagree about the same detached buffer. Other views over the same
/// store are NOT walked: nothing indexes them by buffer, and their reads
/// already answer `undefined` for a window that is no longer there.
pub fn detach_buffer(value: u64) -> bool {
    with_current(|context| {
        let Some(cell) = store_cell(context, value) else {
            return false;
        };
        if !context.detach_buffer(cell) {
            return false;
        }
        if let Some(view) = Value(value).as_slot()
            && let Some(held) = context.views.get_mut(view)
        {
            held.offset = 0;
            held.length = 0;
            super::stamp(context, view, "byteLength", 0.0);
            super::stamp(context, view, "length", 0.0);
        }
        true
    })
}

/// Whether a value is an `ArrayBuffer` that has been detached.
///
/// False for everything that is not one, which is what the ABI wants: the
/// question is "may I read this", and a number is not a detached buffer.
pub fn buffer_detached(value: u64) -> bool {
    with_current(|context| match store_cell(context, value) {
        Some(cell) => context.buffer_detached(cell),
        None => false,
    })
}
