//! Three yes/no facts about a value that `console.log` prints differently, and
//! that no property read can answer: a class (as against a plain function), a
//! wrapper object's primitive, and a regular expression.
//!
//! Each is a lookup in a table the context already keeps for the language's own
//! use — `callables`' class flag, `boxed`, `regexp_at` — so this is the public
//! door onto them, not a second record.

use super::with_current;
use crate::value::Value;

/// Whether `value` is a function declared with `class`.
pub fn is_class_constructor(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.is_class_constructor(cell))
    })
}

/// The primitive a `new Number(1)` / `Object(1n)` wrapper stands for.
pub fn boxed_primitive(value: u64) -> Option<u64> {
    with_current(|context| Value(value).as_slot().and_then(|cell| context.boxed_at(cell)))
}

/// Whether `value` carries a regular expression's internal slot.
pub fn is_regexp_object(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.regexp_at(cell).is_some())
    })
}

/// Whether any key of `value` is an accessor rather than a data property — the
/// cheap question that lets a printer skip asking each key for its descriptor
/// on the common object that has none.
pub fn has_accessors(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.records_accessors(cell))
    })
}

/// The runs of missing elements in an array below `length`, as
/// `(first index, count)` — `[1,,3]` is `[(1, 1)]` and `new Array(3)` is
/// `[(0, 3)]`. Whatever lies past the stored elements is one trailing run, so a
/// sparse `length` of a billion is one pair rather than a billion.
pub fn array_hole_runs(value: u64, length: usize) -> Vec<(usize, usize)> {
    with_current(|context| {
        let Some(cell) = Value(value).as_slot() else {
            return Vec::new();
        };
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let stored = context.elements_at(cell).map_or(0, |elements| elements.len());
        if let Some(elements) = context.elements_at(cell) {
            for (index, &held) in elements.iter().enumerate().take(length) {
                if super::array::is_hole(context, held) {
                    match runs.last_mut() {
                        Some((start, count)) if *start + *count == index => *count += 1,
                        _ => runs.push((index, 1)),
                    }
                }
            }
        }
        if length > stored {
            match runs.last_mut() {
                Some((start, count)) if *start + *count == stored => *count += length - stored,
                _ => runs.push((stored, length - stored)),
            }
        }
        runs
    })
}
