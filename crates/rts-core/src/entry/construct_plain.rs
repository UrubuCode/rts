//! `new C(…)` for a class that needs none of what construction keeps in step.
//!
//! # What this removes, measured
//!
//! `new E()` on a class with no fields cost 88 to 92 ns where an object literal
//! that escapes costs 49 (release, 2026-09-29, `KEEP = …` so that neither is
//! removed): the allocation is the same, and the 40 between them is
//! [`super::functions::construct`] — two argument stacks pushed and popped, the
//! walk that decides whether `new` may reach the callee, the callee resolved a
//! second and a third time, and nine borrows of the context where this takes
//! two. The fields are not where the cost is: four of them add 10 ns.
//!
//! # Which construction is plain
//!
//! Two facts, both read from the record the door already reads:
//!
//! - a **class constructor**. `new` may reach one by definition, so the
//!   constructible walk has nothing to decide;
//! - **light**, `light_call`'s word: nothing in the body reads the argument
//!   record or `new.target`, so the stacks that exist for those questions are
//!   not pushed.
//!
//! A DERIVED class is plain on the same two facts, and is handed no object: its
//! `super()` makes one, from the target pushed here. Refusing it instead was
//! measured — the probe that fails cost a derived construction 10 ns (174 to
//! 188, release, alternating binaries) and bought it nothing.
//!
//! Anything else answers `None` and takes the door as it was, with every
//! refusal it makes and every message it spells.
//!
//! # What is kept although the body cannot ask
//!
//! The target is still pushed. `prototype_for_new` and `super()` read the top
//! of that stack, and a construction that pushed nothing would let them read
//! the target of whatever construction is running outside this one.

use super::functions::{Compiled, allocate_for};
use super::with_current;
use crate::value::Value;

/// The construction, or `None` where it is not plain.
pub(super) fn construct_plain(callee: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> Option<u64> {
    let (code, environment, this, derived) = with_current(|context| {
        let cell = Value(callee).as_slot()?;
        let (code, environment, class) = context.callable_record_at(cell)?;
        if !class || !context.is_light(code) {
            return None;
        }
        let derived = context.is_derived(cell);
        let this = match derived {
            true => super::objects::undefined_of(context),
            false => allocate_for(context, callee, callee)?,
        };
        let depth = context.callees.len();
        context.new_targets.push((callee, depth));
        context.callees.push(callee);
        Some((code, environment, this, derived))
    })?;

    // SAFETY: `functions::invoke`'s argument, unchanged — the address came from
    // the closure record of a cell at the closure layout.
    let entry: Compiled = unsafe { std::mem::transmute::<u64, Compiled>(code) };
    let produced = entry(environment, this, a0, a1, a2, a3);
    let produced = super::tail_call::settle(produced);

    // A constructor that returned an object produced THAT; anything else leaves
    // the fresh one — `construct`'s own rule, asked in the borrow that pops.
    let returned = with_current(|context| {
        context.callees.pop();
        context.new_targets.pop();
        super::primitive::is_object_in(context, produced)
    });
    if returned {
        return Some(produced);
    }
    // `construct`'s rule for a derived constructor, which has no fresh object
    // to fall back on: it may answer `undefined` and nothing else. `this` is
    // that `undefined` here.
    if derived && !super::throw::in_flight() && produced != this {
        super::throw::type_error("Derived constructors may only return object or undefined");
    }
    Some(this)
}
