//! The two traps that intercept a CALL rather than a property.
//!
//! # Why calling a proxy arrives here rather than at a check
//!
//! A proxy is not callable in the sense `functions::resolve` means — it has no
//! code address, and it must not have one: the address is the one thing a
//! program may never choose. So calling one arrives at the point `invoke` finds
//! nothing to jump to, rather than at a test before every jump that every
//! ordinary call in the program would pay for.
//!
//! # Why the arguments are read back rather than taken from the slots
//!
//! The calling convention carries four, and the trap is handed an ARRAY of what
//! the call really passed. Reading the four slots therefore lost the fifth
//! argument onwards and reported the padding as real ones: `add(2, 3)` reached
//! its `apply` trap as `[2, 3, undefined, undefined]`, which a handler joining
//! them printed as `add(2,3,,)`.
//!
//! [`crate::entry::array_proto::arguments_at`] is the one place that says what a
//! call carried — the runtime holds the vector when there are more than four,
//! and drops the trailing padding when there are not. Asking it here is what
//! makes both ends of that rule one rule.

use crate::entry::{array_proto, functions, modules, throw, with_current};
use crate::value::Value;

/// `handler.apply(target, thisArg, args)`, or a call to the target.
pub(in crate::entry) fn apply(object: u64, this: u64, arguments: [u64; 4]) -> Option<u64> {
    let trap = super::trap_for(object, "apply")?;
    if trap.refused {
        return Some(super::absent());
    }
    let listed = passed(arguments);
    let Some(callee) = trap.callee else {
        // The target may be a proxy of its own, and `call_with_args` is what
        // asks that question again — this is the forwarding every absent trap
        // does, through the vector spelling so that a call with more than four
        // arguments keeps them.
        return Some(functions::call_with_args(trap.target, this, listed));
    };
    Some(functions::call(
        callee,
        trap.handler,
        trap.target,
        this,
        listed,
        super::absent(),
    ))
}

/// `handler.construct(target, args, newTarget)`, or `new` on the target.
///
/// # What `newTarget` is here, and what it is not
///
/// The proxy itself, which is what `new P()` means and what the trap in every
/// such program compares against. `Reflect.construct(P, args, other)` should
/// hand the trap `other` instead, and cannot yet: the target stack carries one
/// class per construction and `Reflect.construct` drops its third argument
/// before this is reached. Named rather than silently answered wrong — the gap
/// is in `functions::construct_with_args`, not here.
pub(in crate::entry) fn construct(object: u64, arguments: [u64; 4]) -> Option<u64> {
    let trap = super::trap_for(object, "construct")?;
    if trap.refused {
        return Some(super::absent());
    }
    let listed = passed(arguments);
    let Some(callee) = trap.callee else {
        return Some(functions::construct_with_args(trap.target, listed));
    };
    let answered = functions::call(
        callee,
        trap.handler,
        trap.target,
        listed,
        object,
        super::absent(),
    );
    if throw::in_flight() {
        return Some(answered);
    }
    // `new` produces an object. A trap answering anything else would give the
    // expression a value the language says it cannot have — and unlike a
    // constructor's own `return 1`, which is simply ignored, there is no fresh
    // object here to fall back to.
    if Value(answered).as_slot().is_none() {
        throw::type_error("'construct' on proxy: trap returned non-object");
    }
    Some(answered)
}

/// What the call carried, as the array a trap is handed.
fn passed(arguments: [u64; 4]) -> u64 {
    let given = with_current(|context| array_proto::arguments_at(context, 0, arguments));
    modules::make_array(given)
}
