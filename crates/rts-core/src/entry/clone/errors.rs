//! What survives copying an error, for each of the two readers.
//!
//! # Why both halves are here
//!
//! The walk decides WHAT an error becomes and the build makes it, and every
//! other kind splits cleanly along that line. This one does not: the rule is a
//! single sentence per reader, and it is stated once here, with the build doing
//! only what that sentence already decided.
//!
//! # The clone: class, message, stack — nothing else
//!
//! `err.code = "ENOENT"` does not survive `structuredClone`, and neither does
//! anything a subclass wrote — the HTML specification serialises an error as
//! its class, its message and its stack, and nothing else. Measured against Bun
//! and Node rather than assumed, because it is the surprising half: `Object.keys`
//! on a cloned error is empty in both.
//!
//! # The pickle: everything, and the class it really is
//!
//! A pickle is a file a program reads back, so it keeps what the program put
//! there: `cause`, every own enumerable property, and the class — including a
//! class the program declared that extends `Error`, which the clone flattens to
//! the nearest standard name.

use super::super::objects::undefined_of;
use super::super::Context;
use super::{ClassName, ErrorClass, Policy};
use crate::object::Key;
use crate::text::Str;

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

/// The error classes the pickle knows by name: the seven, and the one the
/// clone leaves out because the HTML specification does.
const PICKLED: [&str; 8] = [
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
];

/// An error, read inside one borrow: its class and the members to walk.
///
/// The texts the clone keeps go into the arena as text, because the clone
/// converts a non-string message with `ToString` and the converted text is not
/// a value anything on the heap holds. The pickle keeps the VALUES, which the
/// walk visits like any other member.
pub(super) struct Read {
    pub(super) class: ErrorClass,
    pub(super) message: Option<Member>,
    pub(super) stack: Option<Member>,
    pub(super) cause: Option<u64>,
    pub(super) extra: Vec<(Key, u64)>,
}

/// One of the two texts, as the policy keeps it.
pub(super) enum Member {
    Text(Str),
    Value(u64),
}

/// Reads an error.
///
/// # Why the reads do not run a getter
///
/// [`super::super::objects::read_property`] walks the chain for a DATA
/// property and answers nothing for an accessor, which is what keeps this inside
/// the borrow the caller already holds. The specification's `Get` would run a
/// getter; an error whose `message` is one is copied with no message here, and
/// that is the stated divergence rather than an oversight.
pub(super) fn read(context: &mut Context, value: u64, cell: u32, policy: Policy) -> Option<Read> {
    match policy {
        Policy::Clone => {
            let spelled = text_at(context, cell, "name").and_then(|text| text.to_rust());
            Some(Read {
                class: ErrorClass::Builtin(
                    spelled
                        .and_then(|spelled| STANDARD.into_iter().find(|known| *known == spelled))
                        .unwrap_or("Error"),
                ),
                message: text_at(context, cell, "message").map(Member::Text),
                stack: text_at(context, cell, "stack").map(Member::Text),
                cause: None,
                extra: Vec::new(),
            })
        }
        Policy::Pickle => {
            let own = |context: &mut Context, name: &str| {
                let key = context.well_known(name);
                super::super::objects::own_property(context, cell, key).map(|found| found.bits())
            };
            let message = own(context, "message").map(Member::Value);
            let stack = own(context, "stack").map(Member::Value);
            let cause = own(context, "cause");
            let extra = super::members::data(context, value, cell, false, true)?;
            Some(Read {
                class: class_of(context, cell),
                message,
                stack,
                cause,
                extra,
            })
        }
    }
}

/// Which class an error is, for the pickle: a declared one if its prototype's
/// constructor is one, else the nearest standard error on its chain.
fn class_of(context: &mut Context, cell: u32) -> ErrorClass {
    let key = context.well_known("constructor");
    let declared = super::super::objects::read_property(context, cell, key)
        .and_then(|found| found.as_slot())
        .and_then(|constructor| super::super::pickle::names::declared_as(context, constructor));
    if let Some(declared) = declared {
        return ErrorClass::Declared(declared);
    }
    let Some(mut at) = super::super::objects::inherited_from(context, cell) else {
        return ErrorClass::Builtin("Error");
    };
    for _ in 0..super::super::objects::CHAIN_LIMIT {
        let here = crate::value::Value::from_slot(at).bits();
        let known = PICKLED.into_iter().find(|name| {
            super::super::class_support::prototype(context, name) == Some(here)
        });
        if let Some(known) = known {
            return ErrorClass::Builtin(known);
        }
        let Some(next) = super::super::objects::inherited_from(context, at) else {
            break;
        };
        at = next;
    }
    ErrorClass::Builtin("Error")
}

/// An error of the class the walk decided on, with no members yet — the build
/// fills them in its second pass, beside every other container's.
///
/// The prototype comes from the class REGISTRATION and not from the source
/// cell, which is the rule `build::dated` follows and the reason a copy
/// answers to the same methods a fresh one does. `name` is NOT written: it
/// lives on that prototype, so writing it here would put an own property where
/// the language has an inherited one.
pub(super) fn empty(context: &mut Context, class: &ErrorClass) -> u64 {
    let Some(cell) = super::super::native::plain(context) else {
        return undefined_of(context);
    };
    let prototype = match class {
        ErrorClass::Builtin(name) => super::build::class_prototype(context, name)
            .or_else(|| super::build::class_prototype(context, "Error")),
        ErrorClass::Declared(ClassName { prototype, .. }) => Some(*prototype),
    };
    if let Some(prototype) = prototype {
        context.set_prototype(cell, prototype);
    }
    crate::value::Value::from_slot(cell).bits()
}

/// Writes the members an error kept into the one [`empty`] made.
///
/// Each is `hidden` — non-enumerable — as the constructor makes them, except
/// the extras, which were enumerable where they came from.
pub(super) fn fill(
    context: &mut Context,
    cell: u32,
    members: [(&str, Option<u64>); 3],
    extra: &[(Key, u64)],
) {
    for (name, held) in members {
        let Some(held) = held else {
            continue;
        };
        let key = context.well_known(name);
        super::super::objects::put(context, cell, key, held);
        super::super::native::hidden(context, cell, key);
    }
    for (key, held) in extra {
        let key = super::build::named(context, *key);
        super::super::objects::put(context, cell, key, *held);
    }
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
