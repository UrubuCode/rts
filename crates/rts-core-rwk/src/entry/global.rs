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
//! # What this is, and what it is not
//!
//! A scope chain. This is the global OBJECT — `globalThis` is on it, a write to
//! an undeclared name creates a property here, and `typeof` of a name nothing
//! declares reaches it. What it is not is a resolution mechanism: the compiler
//! decides which names come here, and refuses the rest rather than answering
//! `undefined` for every typo.

use super::objects::{read_property, undefined_of};
use super::{Context, with_current};
use crate::object::Key;
use crate::value::Value;

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
        // The error family answers for itself, because the arm here would
        // otherwise name seven registrations differing only in which one they
        // call — and which error classes exist is a fact about that module.
        if let Some(register) = super::error::provided(&text) {
            let made = register(context);
            super::objects::put(context, object, Key::Name(name), made);
            return made;
        }
        // A global FUNCTION, which is a value rather than an object with
        // members — so it is made here rather than by a class registration, and
        // recorded as a property like every other name so that
        // `parseInt === parseInt` is true.
        if let Some(code) = super::global_fns::provided(&text) {
            let made = super::native::callable(context, code);
            super::objects::put(context, object, Key::Name(name), made);
            return made;
        }
        let made = match text.as_str() {
            "RegExp" => super::regex::constructor(context),
            "Math" => super::math::register_math(context),
            "Number" => super::number::register_number(context),
            "Boolean" => super::number::register_boolean(context),
            "Function" => super::function_proto::register_function(context),
            "Reflect" => super::reflect::register_reflect(context),
            "Symbol" => super::symbol::constructor(context),
            "JSON" => super::json::register_json(context),
            "Date" => super::date::register_date(context),
            "Map" => super::collections::register_map(context),
            "Set" => super::collections::register_set(context),
            "WeakMap" => super::collections::register_weak_map(context),
            "WeakSet" => super::collections::register_weak_set(context),
            "String" => super::string::constructor(context),
            "Array" => super::array_proto::constructor(context),
            "Object" => super::object_global::constructor(context),
            // The object itself, which is what makes it a global object rather
            // than a table with globals in it: a program can reach it, read
            // what is on it, and put something there.
            "globalThis" => Value::from_slot(object).bits(),
            _ => return undefined_of(context),
        };
        super::objects::put(context, object, Key::Name(name), made);
        made
    })
}

/// Writes a global, creating it.
///
/// # Why an assignment to an undeclared name creates one
///
/// Because that is what sloppy mode does, and it is the only way a program
/// without a module system introduces a global at all. Strict mode throws
/// instead — a `ReferenceError` this engine cannot raise where a handler could
/// catch it, so the sloppy answer is the one implemented and the strict one is
/// the stated gap.
#[rtse::entry]
pub fn global_set(key: i64, value: u64) -> u64 {
    with_current(|context| {
        let Ok(number) = u32::try_from(key) else {
            return value;
        };
        let Some(name) = context.keys.key(number) else {
            return value;
        };
        if let Some(object) = holder(context) {
            super::objects::put(context, object, Key::Name(name), value);
        }
        value
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
    use crate::value::Singletons;

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
