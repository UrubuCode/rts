//! Reading and writing what an object inherits from.
//!
//! # Why the language needs these and `new` did not
//!
//! `new F()` links a prototype without either of them: it reads `F.prototype` as
//! an ordinary property and the runtime does the linking, because both halves
//! happen inside one operation. A class is the case where they come apart —
//! `class B extends A {}` links `B.prototype` to `A.prototype` and `B` to `A` at
//! *definition* time, from a value the program computed, with no construction
//! anywhere near it.
//!
//! # Why this is not `Object.getPrototypeOf`
//!
//! It is the same operation, and that method is how a program will reach it once
//! there is an `Object` to hang it on. These exist first because the compiler
//! needs them before any program does: a class body is lowered into them.

use super::objects::undefined_of;
use super::with_current;
use crate::value::Value;

/// What an object inherits from — `null` when the chain ends there, `undefined`
/// when it was never given one.
///
/// The two are different and the difference is load-bearing: a class extending
/// `null` produces instances whose chain genuinely stops, where an object that
/// was never linked has one that was never started. Collapsing them would make
/// `class X extends null {}` indistinguishable from a plain object.
#[rtse::entry]
pub fn get_prototype(object: u64) -> u64 {
    if let Some(answered) = super::proxy::prototype_of(object) {
        return answered;
    }
    with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return undefined_of(context);
        };
        match context.prototype_at(cell) {
            Some(found) => found,
            // No link of its own — the same question a property miss asks,
            // which `objects::inherited_from` already answers for every kind
            // of cell (callable, text, array, plain object) including the
            // `Object.prototype`-is-root termination. Answering `undefined`
            // here unconditionally, as this used to, was a second and
            // disagreeing answer to that question: it substituted nothing for
            // a no-extends class's `.prototype` (which never runs
            // `set_prototype`) where property lookup already walked it to
            // `Object.prototype`, and a chain read with
            // `Object.getPrototypeOf` dead-ended one turn short of where a
            // property read on the same object would have found it.
            None => match super::objects::inherited_from(context, cell) {
                Some(proto_cell) => Value::from_slot(proto_cell).bits(),
                // `inherited_from` also answers `None` for the cell that IS
                // the root — `Object.prototype`'s own chain ends here, which
                // is `null`, not "no link recorded".
                None => Value::from_singleton(context.singletons.null).bits(),
            },
        }
    })
}

/// Links an object to what it inherits from, and answers the object.
///
/// Answers the object rather than nothing so that a lowering can chain — which
/// is what a class definition does three times in a row. A link the language
/// refuses leaves the object unchanged and still answers it, which is what
/// `Object.setPrototypeOf` does with everything except the throw — see
/// [`apply_prototype`] for which refusals those are and why the throw is not
/// here.
#[rtse::entry]
pub fn set_prototype(object: u64, prototype: u64) -> u64 {
    apply_prototype(object, prototype);
    object
}

/// The link itself, and whether it was made.
///
/// # Why the verdict is the primitive and the object is the wrapper
///
/// Two operations want two different answers from one act:
/// `Object.setPrototypeOf` answers the object it was given and
/// `Reflect.setPrototypeOf` answers whether it worked. Writing them as two
/// functions meant two copies of what a refusal IS, and the copies did not
/// agree — the `Object` spelling refused nothing at all, so a cycle it built
/// was a chain walk that ran to `CHAIN_LIMIT` on every later property read.
///
/// # The two refusals, and the one this cannot spell
///
/// A **cycle**, because a prototype chain that reaches its own start is not a
/// chain — every walk over it is bounded only by `CHAIN_LIMIT`, so the cost is
/// paid by reads that have nothing to do with the link. And a **non-extensible**
/// object, because refusing to grow includes refusing to be relinked; that is
/// what `Object.preventExtensions` promises and what a program checks
/// `isExtensible` for.
///
/// What is not here is the THROW. `Object.setPrototypeOf` raises a `TypeError`
/// on either refusal where `Reflect.setPrototypeOf` answers `false`, and the
/// two spellings meet in this one function — so raising here would make the
/// `Reflect` form throw, which is exactly what a program uses it to avoid. The
/// throw belongs at the `Object` spelling's own site, in `object_global`.
pub(in crate::entry) fn apply_prototype(object: u64, prototype: u64) -> bool {
    // A proxy answers with its handler's own verdict: a `setPrototypeOf` trap
    // returning `false` is a refusal, and reporting `true` because the call
    // reached an object would be inventing the answer.
    if let Some(answered) = super::proxy::set_prototype_verdict(object, prototype) {
        return answered;
    }
    with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return false;
        };
        let already = context.prototype_at(cell);
        // Relinking to what is already there is not a change, so nothing refuses
        // it — which is what makes `Object.setPrototypeOf(frozen, its own proto)`
        // legal.
        if already == Some(prototype) {
            return true;
        }
        if context.integrity_at(cell).is_some() || makes_cycle(context, cell, prototype) {
            return false;
        }
        context.set_prototype(cell, prototype);
        // And the cell takes the type its NEW link gives it. This is the
        // whole of "changing what an object inherits from invalidates the
        // sites that read through it": the number every inline cache
        // compares changes, so a warmed site asks again. No token, no
        // global, and nothing anyone has to remember to call — the one
        // place a link is set is the one place the type is fixed.
        if let Some(ty) = context.region.type_of(cell)
            && let Some(shape) = context.shape_of(ty)
        {
            let fresh = context.typed_as(shape, Some(prototype)).index() as u32;
            context.region.set_type(cell, fresh);
        }
        true
    })
}

/// Whether linking `cell` to `prototype` would close the chain on itself.
///
/// Walks the OWN links only — `prototype_at` rather than
/// `objects::inherited_from` — because the substituted ones are the shared
/// built-in prototypes, which no program can link back to an object of its own.
/// Following those would walk to `Object.prototype` on every `setPrototypeOf`
/// to answer a question they cannot be part of.
fn makes_cycle(context: &super::Context, cell: u32, prototype: u64) -> bool {
    let mut at = Value(prototype).as_slot();
    for _ in 0..super::objects::CHAIN_LIMIT {
        let Some(current) = at else {
            return false;
        };
        if current == cell {
            return true;
        }
        at = context
            .prototype_at(current)
            .and_then(|next| Value(next).as_slot());
    }
    // A chain already longer than a walk will follow is one no read can reach
    // the end of either, so treating it as a cycle refuses exactly what a
    // reader would experience as one.
    true
}
