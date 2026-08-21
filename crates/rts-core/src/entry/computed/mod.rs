//! Property access whose key the program computed, and the operations that
//! follow from one.
//!
//! # Why these are apart from the named ones
//!
//! `o.x` carries a key the **compiler** resolved: a number, decided while
//! compiling, with no text crossing at all. `o[e]` carries a value that has to
//! become a key while running, which is `ToPropertyKey` and interns text.
//!
//! That is one difference and it reaches everything downstream — `in` and
//! `delete` both take a computed key, and an array element is only reachable
//! through this side. Filing them with the named read would put two answers to
//! "what is a key" in one file.
//!
//! # Why this is a folder
//!
//! It passed the crate's 500-line ceiling when `ToPropertyKey` learned to run
//! `toString`, and the split is along the seam the file already had: what a KEY
//! is, decided here; what MOVES a value, in [`access`]; what answers a
//! QUESTION, in [`query`]. The key resolution stays in this file because both
//! of the others start with it, and a copy in either is the "two answers to
//! what is a key" the paragraph above exists to refuse.

mod access;
mod query;

// The four entry points and their ABI descriptors, under the path every caller
// already names — `super::computed::get_indexed` reads the same from a folder
// as it did from a file, which is what makes the split invisible outside it.
pub use access::{GET_INDEXED_ENTRY, SET_INDEXED_ENTRY, get_indexed, set_indexed};
pub use query::{
    DELETE_PROPERTY_ENTRY, HAS_PROPERTY_ENTRY, WITH_HAS_ENTRY, delete_property, has_property,
    with_has,
};
pub(in crate::entry) use query::{delete_own, length_key, remove_own};

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::object::Key;
use crate::value::Value;

/// The key a value names, through `ToPropertyKey`.
///
/// # Why every key becomes a name, including one that spells an index
///
/// `o[0]` and `o["0"]` are
/// one property, which is what the language says, and the only thing lost is an
/// enumeration order nothing implements. Routing it through `Key::from_str` and
/// letting the `None` through would have made `o[0] = 1; o[0]` read as absent —
/// a wrong program that runs, which is the outcome this whole layer refuses.
///
/// `None` means the key was an object, whose `ToPropertyKey` runs a `toString`
/// — user code an entry point cannot call.
/// The key number a value resolves to, for a caller that has the value and
/// needs the number.
///
/// # Why this exists rather than a second pair of accessor entry points
///
/// `define_getter`/`define_setter` take the key the COMPILER resolved, as a
/// number — which is right, and is why a computed accessor name was refused in
/// both a class body and an object literal: there was no way to turn `{ get [e]()
/// {} }`'s key into one. Two more entry points taking a value would have been a
/// second way to define an accessor, differing from the first in one argument.
///
/// This is the missing step instead: resolve the key here, then use the pair
/// that already exists. `-1` for a value that cannot be a key at all, which the
/// caller turns back into doing nothing rather than into key zero.
#[rtse::entry]
pub fn key_number(key: u64) -> i64 {
    // `{ get [k]() {} }` where `k` is an object: `ToPropertyKey` runs its
    // `toString`, which is user code and cannot run inside the borrow below.
    let key = if with_current(|context| super::primitive::is_object_in(context, key)) {
        super::primitive::to_primitive(key, crate::coerce::Hint::String)
    } else {
        key
    };
    with_current(|context| match property_key(context, Value(key)) {
        Some(Key::Name(named)) => named.index() as i64,
        // An accessor whose key is an array INDEX — `get [0]() {}`. It has no
        // shape key, because an index is an element position rather than a
        // property slot, and `define_accessor` takes the latter. Refused here
        // rather than mapped onto some number, which would define an accessor
        // under a property nobody named.
        Some(Key::Index(_)) | None => -1,
    })
}

pub(super) fn property_key(context: &mut Context, key: Value) -> Option<Key> {
    // A symbol is its own key rather than one derived from text, and it is
    // asked first because `ToString` of a symbol is a `TypeError` in the
    // language — converting it here would give `o[Symbol.iterator]` a key the
    // spelling `o["Symbol(Symbol.iterator)"]` could also reach.
    //
    // Answered from the memo the symbol carries, which is what makes this a
    // load rather than the clone-convert-hash it was: `key_text_of` handed back
    // an owned `String`, `Str::from_str` built a UTF-16 copy of it, and the
    // interner then hashed that copy to reach a number it already held — two
    // heap allocations and a hash per access, for a key text that cannot change
    // after the symbol is minted.
    //
    // This is the same fix, on the same function, that the comment below
    // records for STRING keys and measures at 123x. The symbol arm was left on
    // the slow side of it.
    if let Some(found) = super::symbol::key_of(context, key.bits()) {
        return Some(found);
    }
    // The common case, and the one that was measured: the key is ALREADY a
    // string, so there is nothing to convert — only to look up. Reached without
    // copying the text, which the general path below cannot do because
    // `to_text` has to answer an owned `Str` for the cases that build one.
    //
    // Measured before this existed: a read through a computed key cost 123x a
    // read through a named one, and two heap allocations per access were the
    // difference — this copy, and a second inside the interner.
    if let Some(cell) = key.as_slot()
        && let Some(found) = context.key_of_text_cell(cell)
    {
        return Some(found);
    }
    let text = super::text::to_text(context, key)?;
    Some(Key::Name(context.interner.intern(&text, &mut context.keys)))
}

/// A read on a receiver that is not a cell: a number, a boolean, a symbol or a
/// bigint.
///
/// # Why this is here and not another copy of the named path's cascade
///
/// Because it was going to be one. `objects::get_property` grew this fallback
/// when `(5).toFixed(2)` had to work, and the computed path did not — so the two
/// spellings of one read disagreed about whether a primitive has a prototype.
/// Writing the same cascade a second time here would have fixed that instance
/// and left the next one to be found the same way.
///
/// So the cascade is stated once, and both callers reach it. What is genuinely
/// different between the two paths is upstream of this: which key was resolved,
/// and by whom.
pub(super) fn primitive_found(
    context: &mut Context,
    object: Value,
    key: crate::object::Key,
) -> super::accessor::Found {
    if let Some(answer) = super::primitive_proto::own_property(context, object, key) {
        return super::accessor::Found::Value(answer);
    }
    match super::primitive_proto::prototype_of(context, object) {
        Some(cell) => super::accessor::resolve(context, cell, key),
        None => super::accessor::Found::Value(undefined_of(context)),
    }
}

/// What an access has to settle before it takes the borrow that does the work.
///
/// # Why the two questions are one borrow
///
/// They were two: `proxy::is_proxy` took the context to answer a single table
/// lookup, and every indexed access — the overwhelming majority of which touch
/// no proxy at all — paid a second entry into the thread-local before the one
/// that does the work. Asking both inside one borrow costs a proxy nothing and
/// takes that entry off every array element read and write.
///
/// The object-key question joined them for the same reason rather than as a
/// third entry: `ToPropertyKey` of an object runs `toString`, and asking
/// whether it has to is one comparison where CALLING it is a call.
///
/// `ToPropertyKey` still runs only for a proxy or an object key, which is the
/// point the two notes below already make: formatting a double as text and
/// interning it was running on every indexed access and being thrown away,
/// because the element path treats the index as a number.
enum Resolution {
    /// An ordinary receiver and a key that is already one.
    Ready,
    /// The key is an object. `ToPropertyKey` has `toString` to run first, which
    /// is user code and cannot happen under a borrow.
    Coerce,
    /// The receiver is a proxy, and this is the key its handler is asked with.
    Trap(Key),
}

fn resolved(context: &mut Context, object: u64, key: u64) -> Resolution {
    // Before the trap, which is the order the language states: a handler is
    // asked with the string an object key became, never with the object.
    if super::primitive::is_object_in(context, key) {
        return Resolution::Coerce;
    }
    let Some(cell) = Value(object).as_slot() else {
        return Resolution::Ready;
    };
    if context.proxy_at(cell).is_none() {
        return Resolution::Ready;
    }
    match property_key(context, Value(key)) {
        Some(named) => Resolution::Trap(named),
        None => Resolution::Ready,
    }
}

/// The key the access proceeds with, and the trap key when there is a proxy.
///
/// Shared by the four computed operations so that `o[k]`, `o[k] = v`,
/// `k in o` and `delete o[k]` cannot come to different conclusions about what
/// `k` IS — which is the failure this file's own documentation says the split
/// between named and computed access must never produce.
fn opened(object: u64, key: u64) -> (u64, Option<Key>) {
    match with_current(|context| resolved(context, object, key)) {
        Resolution::Ready => (key, None),
        Resolution::Trap(named) => (key, Some(named)),
        Resolution::Coerce => {
            // No borrow held: `toString` is user code. What it answers is a
            // primitive — and when it is not, `to_primitive` answers the object
            // back unchanged — so the second question can never come back
            // `Coerce` and there is no loop here.
            let key = super::primitive::to_primitive(key, crate::coerce::Hint::String);
            match with_current(|context| resolved(context, object, key)) {
                Resolution::Trap(named) => (key, Some(named)),
                _ => (key, None),
            }
        }
    }
}
