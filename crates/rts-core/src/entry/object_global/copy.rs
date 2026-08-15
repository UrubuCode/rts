//! `Object.assign` — copying own enumerable properties from one object to
//! another.
//!
//! Its own module because [`super`] is at the ceiling this workspace sets, and
//! because this is the half of `Object` that COPIES rather than describes:
//! `{ ...source }` is the same operation under different syntax, and
//! `super::super::objects::object_spread` is where that spelling lives. The two
//! agreeing about what "own enumerable" means — symbols included — is the whole
//! reason the symbol pass below reads the way that one does.

use super::super::{Context, with_current};
use crate::value::Value;

/// An array's elements, the borrow ending here.
///
/// Shared with [`super`], whose `entries` and `fromEntries` read a list the same
/// way. One spelling rather than three, because a second would be a second
/// answer to what an array that is not one answers.
pub(super) fn held(array: u64) -> Option<Vec<u64>> {
    with_current(|context: &mut Context| {
        let cell = Value(array).as_slot()?;
        Some(context.elements_at(cell)?.clone())
    })
}

/// `Object.assign(target, ...sources)` — copies the own keys across.
///
/// Three sources rather than any number, because the arity a call carries is
/// four and the receiver takes one of them. It used to read ONE and the comment
/// beside it claimed a call with more was "refused at the site"; it was not —
/// `Object.assign({}, a, b)` silently dropped `b`, which is the failure mode a
/// merge must not have, because the result looks like a merge.
///
/// Beyond three the arguments genuinely are not here, and that stays a gap
/// rather than a quiet loss: it needs the gathered-arguments path, which is a
/// change to how a native is called and not to this function.
pub(super) extern "C" fn assign(
    _e: u64,
    _this: u64,
    target: u64,
    source: u64,
    a2: u64,
    a3: u64,
) -> u64 {
    for source in [source, a2, a3] {
        // An absent argument arrives as `undefined`, and `undefined` has no own
        // keys — so the same skip serves both the spec's rule (a null or
        // undefined source contributes nothing) and the missing-argument case,
        // without this having to know which it is looking at.
        let Some(found) = held(super::super::array::own_keys(source)) else {
            continue;
        };
        for name in found {
            // Through the ordinary paths on both sides, so a getter on the
            // source runs and a setter on the target runs — which is what
            // `assign` does and what a slot-to-slot copy would have skipped.
            let value = super::super::computed::get_indexed(source, name);
            super::super::computed::set_indexed(target, name, value);
        }
        // `own_keys` never reports a symbol — it is the string enumeration,
        // and a symbol has no string spelling for it to report — so a
        // `[sym]: v` entry used to vanish across `Object.assign` and spread
        // silently, which is a merge that drops data rather than one that
        // says it cannot copy something.
        let symbols = with_current(|context| super::super::array::symbol_keyed(context, source));
        for (key, value) in symbols {
            with_current(|context| {
                if let Some(cell) = Value(target).as_slot() {
                    super::super::objects::put(context, cell, key, value);
                }
            });
        }
    }
    target
}
