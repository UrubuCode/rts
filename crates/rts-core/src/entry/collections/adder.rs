//! Filling a collection from an iterable, through the collection's own adder.
//!
//! # Why a constructor does not write its table directly
//!
//! It did, and three things the language guarantees were not true of it. The
//! adder — `set` for `Map` and `WeakMap`, `add` for `Set` and `WeakSet` — is
//! read off the instance ONCE, so a subclass that overrides it fills through
//! the override and a getter on it is observed exactly once. The iterable is
//! STEPPED, so an entry that is refused closes the iterator at that element
//! rather than after the whole sequence has been drained. And an entry is read
//! as the language reads it — `entry[0]` and `entry[1]`, which is a property
//! get, so `new Map([{ 0: "k", 1: "v" }])` builds the same map `[["k", "v"]]`
//! does.
//!
//! `AddEntriesFromIterable` is the specification's name for the pair half; the
//! member half is the same walk with one argument per step, and they are one
//! function here because the only difference is how many arguments the adder
//! takes. Two copies would be the read-once rule and the close rule written
//! twice.

use crate::entry::iterator::drive;
use crate::entry::{computed, functions, iterate, throw};
use crate::value::Value;

/// How many values one step of the iterable contributes to the adder.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    /// `Map` and `WeakMap`: each element is a `[key, value]` entry.
    Entries,
    /// `Set` and `WeakSet`: each element is a member.
    Members,
}

/// Fills `target` by calling its own adder once per element of `iterable`.
///
/// The adder is read from the TARGET rather than taken from this module,
/// because that read is what a subclass overriding `set` is observed by —
/// `class Counting extends Map { set(k, v) { … } }` fills through its own
/// method, and `new Counting([[1, 1]])` is where the language says so.
///
/// Every call below is user code, so rule 8 of the crate's README applies at
/// each: a throw leaves `functions::call` answering `undefined`, and carrying
/// on would add an entry the program never asked for. What a throw does to the
/// ITERATOR is the other half — `close_abrupt` runs its `return` and keeps the
/// original error, which is `IfAbruptCloseIterator`.
pub(super) fn fill(target: u64, iterable: u64, adder_name: &str, shape: Shape) {
    let adder = drive::read(target, adder_name);
    if throw::in_flight() {
        return;
    }
    if !iterate::callable(adder) {
        throw::type_error(&format!("the collection's {adder_name} is not a function"));
        return;
    }
    let Some(source) = drive::iterator(iterable) else {
        return;
    };
    let nothing = drive::absent();
    loop {
        match drive::step(&source) {
            None => return,
            Some(drive::Step::Done) => return,
            Some(drive::Step::Value(element)) => {
                let arguments = match shape {
                    Shape::Members => (element, nothing),
                    Shape::Entries => {
                        // An entry that is not an object is refused BEFORE it is
                        // read, and the refusal closes the iterator: the
                        // specification spells both, and draining the rest first
                        // is what `new Map([1, 2, 3])` did — three steps and a
                        // `return()` that never ran.
                        if !drive::is_object(element) {
                            throw::type_error("an entry of a Map iterable must be an object");
                            drive::close_abrupt(source.object);
                            return;
                        }
                        let key = computed::get_indexed(element, index(0.0));
                        if throw::in_flight() {
                            drive::close_abrupt(source.object);
                            return;
                        }
                        let value = computed::get_indexed(element, index(1.0));
                        if throw::in_flight() {
                            drive::close_abrupt(source.object);
                            return;
                        }
                        (key, value)
                    }
                };
                functions::call(
                    adder,
                    target,
                    arguments.0,
                    arguments.1,
                    nothing,
                    nothing,
                );
                if throw::in_flight() {
                    drive::close_abrupt(source.object);
                    return;
                }
            }
        }
    }
}

/// `0` and `1` as the values `entry[0]` and `entry[1]` are read with.
///
/// Through [`computed::get_indexed`] rather than a named property read, because
/// that is the one function that answers an ARRAY's element, a plain object's
/// `"0"`, a proxy's trap and a getter alike. A read by interned name would find
/// the plain object's property and miss the array's element, which is the shape
/// almost every `new Map([[k, v]])` in existence is written in.
fn index(at: f64) -> u64 {
    Value::from_f64(at).bits()
}
