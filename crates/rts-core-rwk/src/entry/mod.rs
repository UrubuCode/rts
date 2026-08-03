//! How compiled code reaches this crate.
//!
//! # The boundary is scalars, so the state cannot be a parameter
//!
//! An entry point is `extern "C"` over ABI types, and those are `u64`, `i64`,
//! `i32`, `f64`, `bool` and strings. A `&mut ShapeTree` does not cross that
//! boundary and never will.
//!
//! So an operation that needs the heap cannot *receive* it — it reaches ambient
//! state. That is the decision this module is, and the alternative it rejects is
//! threading a context pointer through every call site: it works, it costs a
//! register and an argument everywhere, and it lets a caller pass the wrong one.
//!
//! # One context per thread, not one per process
//!
//! A global behind a lock would serialise every property read in the program,
//! which is the opposite of what a per-region heap is for. The machine already
//! has [`rts_cranelift::sched::SchedulerId`] per region and
//! `Delivery::Elsewhere` for what crosses; a thread-local context is the same
//! shape on the data side.
//!
//! # What qualifies as an entry point
//!
//! The machine's own rule, unchanged:
//!
//! > An entry point exists if and only if the operation touches the heap, the
//! > operating system, or global mutable state. Pure computation is
//! > instructions.
//!
//! So `to_int32` is **not** here — it is arithmetic, and belongs in what the
//! lowering emits. `add` is, because two strings joined is an allocation.
//! Declaring the whole crate would put hundreds of rows in a table whose entire
//! argument is that a small closed set beats a large open one.

mod cell;

pub use cell::Cell;

use std::cell::RefCell;

use rts_cranelift::shape::{KeyRegistry, ShapeTree};

use crate::coerce::{Sum, add as add_primitives, number_to_string};
use crate::heap::{Slab, Slot};
use crate::text::{Interner, Str};
use crate::value::{Singletons, Value, strict_equals, to_boolean};

/// Everything a running program's operations need and cannot be handed.
pub struct Context {
    /// Every heap value, of whichever kind.
    ///
    /// One table, not one per kind — the decision recorded in [`crate::heap`]:
    /// the tag space already spends a tag on "reference", and splitting the
    /// payload to re-encode which kind would spend address bits to save a branch
    /// a shape check performs anyway.
    pub cells: Slab<Cell>,
    /// Every layout. The machine's, because there is exactly one.
    pub shapes: ShapeTree,
    /// Where property keys are numbered, shared with the compiler.
    pub keys: KeyRegistry,
    /// Strings that have been used as keys while running.
    pub interner: Interner,
    /// Which singleton number means what, as the language declared it.
    pub singletons: Singletons,
}

impl Context {
    /// A context holding nothing.
    pub fn new(singletons: Singletons) -> Self {
        Context {
            cells: Slab::new(),
            shapes: ShapeTree::new(),
            keys: KeyRegistry::new(),
            interner: Interner::new(),
            singletons,
        }
    }

    /// The text at a slot, if that slot holds a string.
    pub fn text_at(&self, slot: u32) -> Option<&Str> {
        match self.cells.at(Slot(slot)).ok()? {
            Cell::Text(text) => Some(text),
            Cell::Object(_) => None,
        }
    }

    /// Put a string on the heap and return the value naming it.
    pub fn intern_value(&mut self, text: Str) -> Value {
        let slot = self.cells.insert(Cell::Text(text)).slot();
        Value::from_slot(slot.0)
    }

    /// Whether two slots hold equal text.
    ///
    /// What `===` needs and cannot answer alone: two strings are equal when
    /// their text is, however they were allocated, while two objects are equal
    /// only when they are the same object.
    pub fn same_text(&self, left: u32, right: u32) -> bool {
        match (self.text_at(left), self.text_at(right)) {
            (Some(a), Some(b)) => a.same_units(b),
            _ => false,
        }
    }
}

thread_local! {
    /// This thread's context, absent until something installs one.
    static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
}

/// Install a context for this thread, and run something with it.
///
/// Returns the context afterwards so a caller can inspect what a program left
/// behind — which is how the tests below work, and how a host would read a
/// result out.
pub fn with_context<T>(context: Context, body: impl FnOnce() -> T) -> (Context, T) {
    CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
    let value = body();
    let context = CONTEXT.with(|slot| slot.borrow_mut().take());
    (
        context.expect("the context installed above is still installed"),
        value,
    )
}

/// Run something against this thread's context.
///
/// Aborts when there is none. That is not a runtime condition a program can
/// reach — it means compiled code ran before anything installed a heap, which is
/// a broken embedding — and unwinding out of an `extern "C"` frame is undefined
/// behaviour, so there is nothing better to do than say so and stop.
fn with_current<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    CONTEXT.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(context) = borrowed.as_mut() else {
            eprintln!("rts: an entry point ran with no context installed on this thread");
            std::process::abort();
        };
        body(context)
    })
}

/// `a + b`, on values already reduced to primitives.
///
/// An entry point because joining two strings allocates. The caller has already
/// resolved `ToPrimitive` in the order [`crate::coerce::add_operand_order`]
/// states — this cannot do it, because running a `valueOf` is calling.
#[unsafe(no_mangle)]
pub extern "C" fn __rts_add(left: u64, right: u64) -> u64 {
    with_current(|context| {
        let text_of = |value: Value| {
            value
                .as_slot()
                .and_then(|slot| context.text_at(slot))
                .cloned()
        };

        match add_primitives(Value(left), Value(right), text_of) {
            Some(Sum::Number(number)) => Value::from_f64(number).bits(),
            Some(Sum::Text(text)) => context.intern_value(text).bits(),
            // Neither a number nor a string: the caller handed over something
            // still needing ToPrimitive. Answering NaN would be a wrong number;
            // this is a contract violation, and saying so beats inventing one.
            None => Value::from_f64(f64::NAN).bits(),
        }
    })
}

/// `a === b`.
///
/// An entry point because two strings are equal when their *text* is, which
/// needs the heap. Everything else about it is arithmetic.
#[unsafe(no_mangle)]
pub extern "C" fn __rts_strict_equals(left: u64, right: u64) -> bool {
    with_current(|context| strict_equals(Value(left), Value(right), |a, b| context.same_text(a, b)))
}

/// `ToBoolean`.
///
/// An entry point for one case out of seven: the empty string. Every other
/// falsy value is decided by arithmetic, and a lowering that proved its operand
/// is a number should emit the comparison rather than call this.
#[unsafe(no_mangle)]
pub extern "C" fn __rts_to_boolean(value: u64) -> bool {
    with_current(|context| {
        let singletons = context.singletons;
        to_boolean(Value(value), singletons, |slot| {
            context.text_at(slot as u32).is_some_and(Str::is_empty)
        })
    })
}

/// `String(n)`.
///
/// An entry point because the result is allocated.
#[unsafe(no_mangle)]
pub extern "C" fn __rts_number_to_string(value: f64) -> u64 {
    with_current(|context| {
        let text = number_to_string(value);
        context.intern_value(text).bits()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let (_, equal) = with_context(context, || __rts_strict_equals(first.bits(), second.bits()));

        assert!(
            equal,
            "strings compare by text under ===; comparing the reference would \
             make \"a\" === \"a\" false whenever the two were built separately"
        );
    }

    #[test]
    fn two_distinct_objects_are_not_strictly_equal() {
        use crate::object::Object;

        let mut context = fresh();
        let root = context.shapes.root();
        let first = context
            .cells
            .insert(Cell::Object(Object::new(root, vec![], None)));
        let second = context
            .cells
            .insert(Cell::Object(Object::new(root, vec![], None)));

        let left = Value::from_slot(first.slot().0);
        let right = Value::from_slot(second.slot().0);

        let (_, equal) = with_context(context, || __rts_strict_equals(left.bits(), right.bits()));
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
            __rts_add(Value::from_i32(2).bits(), Value::from_i32(3).bits())
        });
        assert_eq!(Value(sum).as_f64(), Some(5.0));

        let number_text = {
            let (mut context, printed) = with_context(context, || __rts_number_to_string(1.0));
            let joined = with_context(context, || __rts_add(text.bits(), printed));
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
                __rts_to_boolean(empty.bits()),
                __rts_to_boolean(filled.bits()),
                __rts_to_boolean(Value::from_i32(0).bits()),
                __rts_to_boolean(Value::from_i32(1).bits()),
            ]
        });

        assert_eq!(answers, [false, true, false, true]);
    }

    #[test]
    fn a_number_prints_through_the_entry_point_as_it_prints_anywhere() {
        let (context, printed) = with_context(fresh(), || __rts_number_to_string(0.1 + 0.2));
        let text = context
            .text_at(Value(printed).as_slot().unwrap())
            .and_then(Str::to_rust);
        assert_eq!(text.as_deref(), Some("0.30000000000000004"));
    }
}
