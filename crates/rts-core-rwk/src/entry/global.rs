//! The names the runtime provides, and where their values live.
//!
//! # Why an object and not a table
//!
//! Because `RegExp.x = 1` is a property write on an ordinary object, and every
//! mechanism for that already exists. A table keyed by name would answer the
//! read and have nothing to say about the write — and the write is not exotic:
//! attaching something to a constructor is how JavaScript has always been
//! extended.
//!
//! Holding them as properties also means the number that crosses is a **key**,
//! from the registry the compiler mints from. A position in a list of provided
//! names would be a second numbering over the same names, which is the mistake
//! this crate keeps refusing.
//!
//! # Why the values are made on demand
//!
//! `RegExp` is three cells: the constructor, its prototype, and the two natives
//! on that prototype. A program that never writes a regular expression should
//! not spend them, and the region is fixed in size — so the object is empty
//! until something asks, and each name is made the first time it is read.
//!
//! # What this is not
//!
//! The global object. There is no `globalThis`, a write does not create a
//! binding, and an unbound name is still refused by the compiler rather than
//! resolved here. This answers *which value does this provided name have*, and
//! nothing else.

use super::objects::{read_property, undefined_of};
use super::{Context, with_current};
use crate::object::Key;

/// The value a provided name has, by the key number the compiler resolved.
///
/// Answers `undefined` for a name this runtime does not supply. The compiler
/// decides which names are readable — that is a fact about JavaScript — and this
/// decides which of them it can actually produce, so the two sets are allowed to
/// differ and the difference is visible as a value rather than as a link error.
#[rtse::entry]
pub fn global_get(key: i64) -> u64 {
    with_current(|context| {
        let Ok(number) = u32::try_from(key) else {
            return undefined_of(context);
        };
        let Some(name) = context.keys.key(number) else {
            // A key nothing minted, which is a compiled program naming a
            // property the host never wired up rather than anything a program
            // can express.
            return undefined_of(context);
        };
        let object = match holder(context) {
            Some(cell) => cell,
            None => return undefined_of(context),
        };
        if let Some(found) = read_property(context, object, Key::Name(name)) {
            return found.bits();
        }
        // Not made yet. Which one it is has to come from the text, because a key
        // number is issued by interning and carries no name of its own.
        let Some(text) = context.interner.text(name).and_then(|text| text.to_rust()) else {
            return undefined_of(context);
        };
        let made = match text.as_str() {
            "RegExp" => super::regex::constructor(context),
            "String" => super::string::constructor(context),
            _ => return undefined_of(context),
        };
        super::objects::put(context, object, Key::Name(name), made);
        made
    })
}

/// The object the provided names are properties of, made once.
fn holder(context: &mut Context) -> Option<u32> {
    if let Some(made) = context.globals {
        return Some(made);
    }
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    let cell = context.region.alloc(crate::heap::STRIDE, ty)?;
    context.globals = Some(cell);
    Some(cell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::with_context;
    use crate::value::{Singletons, Value};

    /// A context installed for the duration, with keys already issued.
    fn hosted<T>(body: impl FnOnce() -> T) -> T {
        let singletons = Singletons {
            undefined: 0,
            null: 1,
        };
        let context = Context::new(singletons);
        with_context(context, body).1
    }

    /// The number a name has, minted the way a host mints it.
    fn key_of(name: &str) -> i64 {
        with_current(|context| {
            let text = crate::text::Str::from_str(name);
            context.interner.intern(&text, &mut context.keys).index() as i64
        })
    }

    #[test]
    fn one_name_read_twice_is_one_object() {
        // `RegExp === RegExp` is true, and a value made per read would make it
        // false — which is not a slow answer but a wrong one, since a program
        // attaching a property to the constructor would attach it to a copy
        // nothing else sees.
        hosted(|| {
            let key = key_of("RegExp");
            let first = global_get(key);
            let second = global_get(key);
            assert_eq!(first, second);
            assert!(Value(first).as_slot().is_some(), "an object, not undefined");
        });
    }

    #[test]
    fn a_name_this_runtime_does_not_supply_reads_as_undefined() {
        // The compiler decides which names are readable and this decides which
        // it can produce. The two sets are allowed to differ, and the
        // difference has to be a value rather than a crash.
        hosted(|| {
            let produced = global_get(key_of("Elephant"));
            let expected =
                rts_cranelift::tags::encode(rts_cranelift::tags::TAG_SINGLETON, 0);
            assert_eq!(produced, expected);
        });
    }
}
