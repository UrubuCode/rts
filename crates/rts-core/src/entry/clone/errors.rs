//! What survives cloning an error, and what does not.
//!
//! # Why both halves are here
//!
//! The walk decides WHAT an error clones as and the build makes it, and every
//! other kind splits cleanly along that line. This one does not: the rule is a
//! single sentence — *name if it is one of seven, message, stack, nothing else*
//! — and it is stated once, in [`node_for`], with [`made`] doing only what that
//! sentence already decided. Split across the two modules, the list of seven
//! would sit in one and the fallback for a name outside it in the other.
//!
//! # What is dropped, and why that is the answer rather than a gap
//!
//! Every other own property. `err.code = "ENOENT"` does not survive, and
//! neither does anything a subclass wrote — the HTML specification serialises
//! an error as its class, its message and its stack, and nothing else. Measured
//! against Bun and Node rather than assumed, because it is the surprising half:
//! `Object.keys` on a cloned error is empty in both.

use super::super::objects::undefined_of;
use super::super::{Context, with_current};
use super::Node;
use crate::text::Str;
use crate::value::Value;

/// The seven names an error may clone as.
///
/// The HTML specification lists them, and the list is the whole of the rule: a
/// `name` outside it — a subclass's, an `AggregateError`'s, one a program
/// assigned — clones as `"Error"`. Written out rather than derived from the
/// class registry, which holds `AggregateError` too and would therefore
/// disagree with both Bun and Node about the one case they were checked on.
const STANDARD: [&str; 7] = [
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
];

/// What an error clones as, read inside one borrow.
///
/// # Why the three reads do not run a getter
///
/// [`super::super::objects::read_property`] walks the chain for a DATA property
/// and answers nothing for an accessor, which is what keeps this inside the
/// borrow the caller already holds. The specification's `Get` would run a
/// getter; an error whose `message` is one clones with no message here, and
/// that is the stated divergence rather than an oversight. It buys the same
/// thing the `Date` probe beside it buys — no call, so no rule-8 question about
/// an answer that may never have happened.
pub(super) fn node_for(context: &mut Context, cell: u32) -> Node {
    let spelled = text_at(context, cell, "name").and_then(|text| text.to_rust());
    Node::Error {
        class: spelled
            .and_then(|spelled| STANDARD.into_iter().find(|known| *known == spelled))
            .unwrap_or("Error"),
        message: text_at(context, cell, "message"),
        stack: text_at(context, cell, "stack"),
    }
}

/// An error of the class the walk decided on, carrying its two texts.
///
/// The prototype comes from the class REGISTRATION and not from the source
/// cell, which is the rule `clone::build::dated` follows and the reason a clone
/// answers to the same methods a fresh one does. `name` is NOT written: it
/// lives on that prototype, so writing it here would put an own property where
/// the language has an inherited one — `Object.keys` on a cloned error would
/// then report a key no engine reports.
///
/// The fallback to `"Error"` is not defensive. [`node_for`] only ever names one
/// of the seven, and six of those are registered lazily — a program that cloned
/// a `TypeError` has certainly reached `TypeError` — but an error carrying the
/// general prototype is a better answer than one carrying none.
pub(super) fn made(
    context: &mut Context,
    class: &'static str,
    message: Option<&Str>,
    stack: Option<&Str>,
) -> u64 {
    let Some(cell) = super::super::native::plain(context) else {
        return undefined_of(context);
    };
    if let Some(prototype) = super::super::class_support::prototype(context, class)
        .or_else(|| super::super::class_support::prototype(context, "Error"))
    {
        context.set_prototype(cell, prototype);
    }
    for (name, text) in [("message", message), ("stack", stack)] {
        let Some(text) = text else {
            continue;
        };
        let key = context.well_known(name);
        let value = context.intern_value(text.clone()).bits();
        super::super::objects::put(context, cell, key, value);
    }
    Value::from_slot(cell).bits()
}

/// One own-or-inherited data property, as the text `ToString` would make of it.
///
/// `None` for an absent property AND for one holding `undefined`, which the
/// caller needs kept apart from the empty string: `new Error()` has no message
/// at all, and cloning it into `message: ""` would give the copy an own
/// property the original never had.
fn text_at(context: &mut Context, cell: u32, name: &str) -> Option<Str> {
    let key = context.well_known(name);
    let found = super::super::objects::read_property(context, cell, key)?;
    if found.bits() == undefined_of(context) {
        return None;
    }
    super::super::text::to_text(context, found)
}

/// The node an error becomes, from outside a borrow — which is where
/// [`super::walk`] stands.
pub(super) fn walked(cell: u32) -> Node {
    with_current(|context| node_for(context, cell))
}
