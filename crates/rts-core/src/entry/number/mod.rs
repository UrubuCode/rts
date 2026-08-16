//! `Number` and `Boolean`: the conversions, the facts about a double, and the
//! two parsers a program reaches under two names.
//!
//! Three modules and one registration. [`class`] holds what `#[rtse::class]`
//! declares, [`format`] writes a number in the shapes the shortest decimal is
//! not, and [`parse`] reads one out of text. The registration is here because it
//! is the one thing none of the three can do alone: `Number.parseInt` is the
//! same function object as the global `parseInt`, so installing it means
//! reaching the global object.

mod class;
mod format;
mod parse;

// The declared views. `entry::declared` reads the two type lists and
// `entry::global` reads `register_boolean`; both name them through this module,
// so the split behind it is not something either has to know about.
pub(in crate::entry) use class::{BOOLEAN_TYPES, NUMBER_TYPES, register_boolean};
// What `global_fns` needs to give `parseInt` and `parseFloat` a body, and what
// `bigint_class` reads for its own `toString(radix)`.
pub(super) use parse::{float_of, integer_prefix, leading, radix_of};

use super::Context;
use crate::value::Value;

/// Installs `Number`, and makes its two parsing statics the SAME function
/// objects the global names read.
///
/// # Why they are not `#[stat]` members
///
/// Because `Number.parseInt === parseInt` is `true` in the language, and a
/// program can see it — the fixture that found this compares them. Two natives
/// with the same body are two cells, which compares `false`; it is also two
/// bodies, which is where one of them comes to read a leading sign differently.
/// So there is one implementation, in [`super::global_fns`] where the global
/// list that owns it is, and one cell, which this hangs on the constructor.
///
/// The alternative was for `global.rs` to read the constructor's copy when the
/// global name is first read. That inverts the dependency — the global object
/// would have to know which class supplies which of its own names — and it
/// leaves the identity depending on which of the two a program touches first.
pub(in crate::entry) fn register_number(context: &mut Context) -> u64 {
    let made = class::register_number(context);
    if let Some(cell) = Value(made).as_slot() {
        for name in ["parseInt", "parseFloat"] {
            shared_with_global(context, cell, name);
        }
    }
    made
}

/// Puts the global function `name` onto `cell` under the same name, making it
/// when nothing has read the global yet.
///
/// # Why the global object is where the one cell lives
///
/// Because which spelling a program reaches first is the program's choice.
/// `parseInt(x)` before `Number.parseInt` reaches the lazy making in
/// [`super::global`]; `Number.parseInt` before `parseInt` reaches this. Making
/// the global object the holder in both directions is what leaves ONE cell
/// either way — this reads it when it is there and seeds it when it is not, and
/// the lazy path then finds it as an ordinary property and never makes a second.
///
/// Idempotent, which matters because a registration answers early when the
/// class is already made: the second pass reads back what the first wrote.
///
/// The name is not written on the callable, because [`super::global`] does not
/// write one either. Answering a different `.name` depending on which spelling
/// was read first is the disagreement this function exists to remove.
fn shared_with_global(context: &mut Context, cell: u32, name: &str) {
    let Some(holder) = super::global::holder(context) else {
        return;
    };
    let key = context.well_known(name);
    let shared = match super::objects::read_property(context, holder, key) {
        Some(found) => found.bits(),
        None => {
            let Some((code, arity)) = super::global_fns::provided(name) else {
                return;
            };
            let made = super::native::callable(context, code);
            // `name` and `length`, exactly as `super::global` writes them on the
            // same cell reached from the other side. Written in both places
            // because either one may be the FIRST to make it, and a function
            // whose description depended on which spelling a program touched
            // first is the identity bug this whole function exists to avoid.
            super::native::name_of(context, made, name);
            super::native::length_of(context, made, arity);
            super::objects::put(context, holder, key, made);
            made
        }
    };
    super::objects::put(context, cell, key, shared);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{objects, with_context, with_current};
    use crate::value::Singletons;

    /// A context installed for the duration, the way a host installs one.
    fn hosted<T>(body: impl FnOnce() -> T) -> T {
        let singletons = Singletons {
            undefined: 0,
            null: 1,
            hole: 2,
        };
        let context = Context::new(singletons, crate::value::Kinds::in_declaration_order());
        with_context(context, body).1
    }

    /// What the global object holds under a name, if anything.
    fn global(name: &str) -> Option<u64> {
        with_current(|context| {
            let holder = super::super::global::holder(context)?;
            let key = context.well_known(name);
            objects::read_property(context, holder, key).map(|found| found.bits())
        })
    }

    /// What the constructor holds under a name.
    fn on_constructor(made: u64, name: &str) -> Option<u64> {
        with_current(|context| {
            let cell = Value(made).as_slot()?;
            let key = context.well_known(name);
            objects::read_property(context, cell, key).map(|found| found.bits())
        })
    }

    #[test]
    fn number_parse_int_is_the_global_parse_int() {
        // Identity, not agreement: `Number.parseInt === parseInt` is `true` in
        // the language, and two natives with the same body would compare
        // `false` while passing every test that only checks what they answer.
        hosted(|| {
            let made = with_current(register_number);
            for name in ["parseInt", "parseFloat"] {
                let installed = on_constructor(made, name);
                assert_eq!(
                    installed,
                    global(name),
                    "{name} on the constructor and on the global object must be \
                     one cell"
                );
                assert!(installed.is_some(), "{name} was not installed at all");
            }
        });
    }

    #[test]
    fn the_global_read_first_is_the_one_the_constructor_gets() {
        // The other order, which is the one that breaks if the constructor
        // makes its own and the global object later finds a property already
        // there: whichever runs first must be what the second answers.
        hosted(|| {
            let seeded = with_current(|context| {
                let holder = super::super::global::holder(context).expect("a global object");
                let key = context.well_known("parseInt");
                let (code, _) = super::super::global_fns::provided("parseInt").expect("a body");
                let made = super::super::native::callable(context, code);
                objects::put(context, holder, key, made);
                made
            });
            let made = with_current(register_number);
            assert_eq!(on_constructor(made, "parseInt"), Some(seeded));
        });
    }
}
