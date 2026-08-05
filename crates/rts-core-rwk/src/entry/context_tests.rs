//! The context, exercised without a compiled program in sight.
//!
//! Its own file because `mod.rs` holds the type and these hold what it
//! guarantees, and the crate's rule 6 stops a file at five hundred lines — the
//! type's documentation alone is most of that, because every field here is a
//! decision with an alternative.

use super::*;
use crate::value::Value;

fn singletons() -> Singletons {
    Singletons {
        undefined: 0,
        null: 1,
    }
}

fn fresh() -> Context {
    Context::new(singletons())
}

#[test]
fn two_separately_allocated_strings_are_strictly_equal() {
    let mut context = fresh();
    let first = context.intern_value(Str::from_str("a"));
    let second = context.intern_value(Str::from_str("a"));
    assert_ne!(first.bits(), second.bits(), "different slots");

    let (_, equal) = with_context(context, || strict_equals(first.bits(), second.bits()));

    assert!(
        equal,
        "strings compare by text under ===; comparing the reference would \
             make \"a\" === \"a\" false whenever the two were built separately"
    );
}

#[test]
fn two_distinct_objects_are_not_strictly_equal() {
    let mut context = fresh();
    // Two cells in the region, which is where an object's identity is now.
    let root = context.shapes.root();
    let ty = context.layout_of(root).index() as u32;
    let first = context.region.alloc(crate::heap::STRIDE, ty).expect("room");
    let second = context.region.alloc(crate::heap::STRIDE, ty).expect("room");

    let left = Value::from_slot(first);
    let right = Value::from_slot(second);

    let (_, equal) = with_context(context, || strict_equals(left.bits(), right.bits()));
    assert!(
        !equal,
        "objects compare by identity, which is exactly what strings do not"
    );
}

#[test]
fn adding_two_numbers_stays_a_number_and_adding_a_string_allocates() {
    let mut context = fresh();
    let text = context.intern_value(Str::from_str("n="));

    let (context, sum) = with_context(context, || {
        add(Value::from_i32(2).bits(), Value::from_i32(3).bits())
    });
    assert_eq!(Value(sum).as_f64(), Some(5.0));

    let number_text = {
        let (mut context, printed) = with_context(context, || number_to_string(1.0));
        let joined = with_context(context, || add(text.bits(), printed));
        context = joined.0;
        let value = Value(joined.1);
        context
            .text_at(value.as_slot().unwrap())
            .and_then(Str::to_rust)
    };
    assert_eq!(number_text.as_deref(), Some("n=1"));
}

#[test]
fn the_empty_string_is_the_one_falsy_value_that_needs_the_heap() {
    let mut context = fresh();
    let empty = context.intern_value(Str::empty());
    let filled = context.intern_value(Str::from_str("x"));

    let (_, answers) = with_context(context, || {
        [
            to_boolean(empty.bits()),
            to_boolean(filled.bits()),
            to_boolean(Value::from_i32(0).bits()),
            to_boolean(Value::from_i32(1).bits()),
        ]
    });

    assert_eq!(answers, [false, true, false, true]);
}

#[test]
fn a_number_prints_through_the_entry_point_as_it_prints_anywhere() {
    let (context, printed) = with_context(fresh(), || number_to_string(0.1 + 0.2));
    let text = context
        .text_at(Value(printed).as_slot().unwrap())
        .and_then(Str::to_rust);
    assert_eq!(text.as_deref(), Some("0.30000000000000004"));
}
