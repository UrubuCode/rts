//! A fresh object that inherits from a prototype it is born with.
//!
//! # What this removes, measured
//!
//! `{ __proto__: p, a: i }` cost 540 ns where `{ a: i }` costs 48 (release,
//! 2026-09-29). The literal was built as an ordinary object and RELINKED:
//! `set_prototype` on a cell that already has a type is a change to a chain,
//! so it retypes the cell and tells every cache that had read an absence
//! through it — once per object, for an object nothing had read yet.
//!
//! An object born under its prototype has no chain to change. It is typed by
//! what it inherits from before it exists, which is what
//! `functions::allocate_for` does for `new`, and for the same reason: two
//! objects with the same fields under different prototypes must not share a
//! type, or a site warmed on one reads the other's methods.
//!
//! # What is left to the relink
//!
//! Only an OBJECT takes the short road. `null` makes an object with no
//! prototype and anything else is ignored by the language; both are
//! `chain::apply_prototype`'s to decide, on an ordinary object, exactly as
//! before — so there is one statement of what `__proto__: v` means for a `v`
//! that is not an object.

use super::with_current;
use crate::value::Value;

/// A fresh object whose prototype is `prototype`.
#[rtse::entry]
pub fn object_new_under(prototype: u64) -> u64 {
    let born = with_current(|context| {
        if !super::primitive::is_object_in(context, prototype) {
            return None;
        }
        let shape = context.shapes.root();
        let ty = context.typed_as(shape, Some(prototype)).index() as u32;
        let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
        context.set_prototype(cell, prototype);
        Some(Value::from_slot(cell).bits())
    });
    match born {
        Some(object) => object,
        None => {
            let object = with_current(|context| super::objects::object_new_in(context));
            super::chain::apply_prototype(object, prototype);
            object
        }
    }
}
