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
        supply(context, name).unwrap_or_else(|| undefined_of(context))
    })
}

/// Makes the global a key names, when this runtime supplies one.
///
/// # Why a property read has to be able to reach this
///
/// The globals are built LAZILY — each one the first time it is read — so the
/// global object holds only what has been asked for. That is invisible while
/// every read goes through [`global_get`], which asks by name. It stops being
/// invisible the moment a program writes `globalThis.Object`: that is an
/// ordinary property read on an object where nothing has yet made `Object`, and
/// it answered `undefined` for every name the program had not already used
/// under its bare spelling.
///
/// So a miss on the global object comes here, rather than the laziness being
/// given up. Building all of them the first time `globalThis` is touched was
/// the alternative, and it spends every registration on a program that reads
/// one name — which is the cost the laziness exists to refuse.
pub(in crate::entry) fn supply(
    context: &mut Context,
    name: rts_cranelift::shape::Key,
) -> Option<u64> {
    let object = holder(context)?;
    {
        // Which one it is has to come from the text, because a key number is
        // issued by interning and carries no name of its own.
        let text = context.interner.text(name).and_then(|text| text.to_rust())?;
        // The error family answers for itself, because the arm here would
        // otherwise name seven registrations differing only in which one they
        // call — and which error classes exist is a fact about that module.
        if let Some(register) = super::error::provided(&text) {
            let made = register(context);
            super::objects::put(context, object, Key::Name(name), made);
            return Some(made);
        }
        // A global FUNCTION, which is a value rather than an object with
        // members — so it is made here rather than by a class registration, and
        // recorded as a property like every other name so that
        // `parseInt === parseInt` is true.
        if let Some((code, arity)) = super::global_fns::provided(&text)
            .or_else(|| super::uri::provided(&text))
            .or_else(|| super::clone::provided(&text))
        {
            let made = super::native::callable(context, code);
            // `name` and `length`, as `SetFunctionName` and `SetFunctionLength`
            // put them on every function. A global function reached this way had
            // neither, so `parseInt.name` and `Number.parseInt.length` both read
            // `undefined` where a method installed by `#[rtse::class]` answered
            // properly — the same property, absent only because this path made
            // its cell by hand.
            super::native::name_of(context, made, &text);
            super::native::length_of(context, made, arity);
            super::objects::put(context, object, Key::Name(name), made);
            return Some(made);
        }
        let made = match text.as_str() {
            "RegExp" => super::regex::constructor(context),
            "Intl" => super::intl::register_intl_namespace(context),
            "Math" => super::math::register_math(context),
            "Number" => super::number::register_number(context),
            "Boolean" => super::number::register_boolean(context),
            "BigInt" => super::bigint_class::register_big_int_class(context),
            // The species hook is installed here rather than by the class
            // attribute: only seven built-ins have one, so a member every
            // `#[rtse::class]` emitted would be six wrong answers to buy one
            // right one.
            "Promise" => {
                let made = super::promise::register_promise(context);
                super::native::species(context, made);
                made
            }
            // Through `eval`, which is the same registration with the one thing
            // a class declaration cannot give it: what `new Function(…)` runs.
            // See that module for why the constructor is not a `#[construct]`
            // member of `Function` itself.
            "Function" => super::eval::register_function_constructor(context),
            // Not in `global_fns`, although it is a global function: that
            // module holds natives which need nothing from the host, and this
            // one is answerable only because a parser was installed from above.
            "eval" => super::eval::eval_callable(context),
            "Reflect" => super::reflect::register_reflect(context),
            "Symbol" => super::symbol::constructor(context),
            "JSON" => super::json::register_json(context),
            "Date" => super::date::register_date(context),
            "Generator" => super::generator::register(context),
        "Proxy" => super::proxy::register_proxy(context),
        "Map" => super::collections::register_map(context),
            "Set" => super::collections::register_set(context),
            "Buffer" => super::buffer::register_buffer(context),
            "ArrayBuffer" => super::buffers::register_array_buffer(context),
            "DataView" => super::buffers::register_data_view(context),
            "Int8Array" => super::buffers::int8_array(context),
            "Uint8Array" => super::buffers::uint8_array(context),
            "Uint8ClampedArray" => super::buffers::uint8_clamped_array(context),
            "BigInt64Array" => super::buffers::big_int64_array(context),
            "BigUint64Array" => super::buffers::big_uint64_array(context),
            "Int16Array" => super::buffers::int16_array(context),
            "Uint16Array" => super::buffers::uint16_array(context),
            "Int32Array" => super::buffers::int32_array(context),
            "Uint32Array" => super::buffers::uint32_array(context),
            "Float32Array" => super::buffers::float32_array(context),
            "Float64Array" => super::buffers::float64_array(context),
            "WeakMap" => super::collections::register_weak_map(context),
            "WeakSet" => super::collections::register_weak_set(context),
            "WeakRef" => super::collections::register_weak_ref(context),
            "FinalizationRegistry" => super::collections::register_finalization_registry(context),
            "SharedArrayBuffer" => super::buffers::register_shared_array_buffer(context),
            "Atomics" => super::buffers::register_atomics(context),
            "Iterator" => super::iterator::register(context),
            "String" => super::string::constructor(context),
            "Array" => super::array_proto::constructor(context),
            "Object" => super::object_global::constructor(context),
            // The object itself, which is what makes it a global object rather
            // than a table with globals in it: a program can reach it, read
            // what is on it, and put something there.
            "globalThis" => Value::from_slot(object).bits(),
            _ => return None,
        };
        super::objects::put(context, object, Key::Name(name), made);
        Some(made)
    }
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

/// A read of a name the compiler proved is neither declared, provided, nor
/// created by a sloppy-mode write — reached only for that case, unconditionally.
///
/// # Why this exists beside [`global_get`]
///
/// `global_get` answers `undefined` for a name it does not personally supply,
/// and that answer is right there: the compiler decided the NAME is one a
/// program may read (it is in `PROVIDED`), and which of those this host actually
/// built is this crate's question, not the program's mistake. This entry is for
/// the opposite case — `rts-codegen`'s `globals.rs` could not find the name
/// anywhere, in any scope, in `PROVIDED`, or among the names a sloppy write
/// creates — so at THIS call the answer is not "this host lacks it", it is "the
/// language says reading this throws". `ReferenceError`, not `undefined`, is
/// what Node and Bun answer for the same program.
///
/// # Why it always throws rather than checking the holder first
///
/// Because the compiler already checked every way a name could resolve before
/// emitting this call at all — see `rts-codegen::emit::binding::read`. Checking
/// again here would be asking the same question this crate cannot answer better
/// than the one that already asked it, and would let a name a sloppy write
/// *had not run yet* answer `undefined` instead of the same `ReferenceError`
/// Node gives a script that reads before the assignment runs.
#[rtse::entry]
pub fn global_get_unbound(key: i64) -> u64 {
    // Collected and the borrow dropped before raising: `reference_error` opens
    // its own `with_current`, and a second one nested inside this closure's
    // would be a re-entrant borrow of the same context — the abort this
    // crate's rule 8 exists to keep out of an `extern "C"` frame.
    // The GLOBAL OBJECT is the last link of the scope chain, and only a miss
    // THERE is a `ReferenceError`. A name nothing lexical binds may still have
    // been put on it — `globalThis["x"] = f` is how one script publishes to
    // another, and the emitter cannot see that write because it is a computed
    // property assignment rather than a bare one. Raising without asking made
    // `x` unreachable although the program had just defined it.
    if let Some(found) = with_current(|context| {
        let name = u32::try_from(key).ok().and_then(|number| context.keys.key(number))?;
        let object = holder(context)?;
        read_property(context, object, Key::Name(name)).map(|value| value.bits())
    }) {
        return found;
    }
    // Collected and the borrow dropped before raising, for the reason below.
    let text = with_current(|context| {
        let name = u32::try_from(key).ok().and_then(|number| context.keys.key(number));
        name.and_then(|name| context.interner.text(name))
            .and_then(|text| text.to_rust())
    });
    let message = match text {
        Some(text) => format!("{text} is not defined"),
        None => "is not defined".to_owned(),
    };
    super::throw::reference_error(&message);
    with_current(|context| undefined_of(&*context))
    // The `undefined` above is never actually observed: `reference_error` set
    // the pending throw, and every call site this crosses
    // (`rts-codegen::emit::expr::call`) checks for one immediately after the
    // call and re-raises before the value is used.
}

/// `OrdinaryCallBindThis` for a NON-STRICT function: the receiver it was called
/// with, or the global object when it was called with none.
///
/// # Why the runtime and not the emitter
///
/// Because the answer is the global object, which only this crate can produce —
/// it is made on demand by [`holder`] and lives in the context. The emitter's
/// alternative was a branch around a `GlobalGet("globalThis")`, which is the
/// same call plus a comparison the compiler would have to spell in IR; this way
/// the substitution rule is stated once, where the object is.
///
/// # Why only a function the compiler CALLED non-strict reaches this
///
/// Module code is strict, so `this` stays `undefined` there and must: the whole
/// engine is strict, and substituting everywhere would make `this === undefined`
/// answer `false` in code that reads it. `rts-codegen`'s `emit::nonstrict` is
/// what decides, and the only source of a non-strict function here is
/// `Function`/`eval` — text compiled into a running program.
///
/// # What is deliberately absent
///
/// A primitive receiver. The specification wraps one — `Number.prototype.f`
/// called on `1` sees a `Number` OBJECT in sloppy mode — and this crate has no
/// `ToObject`, so a primitive is answered unchanged. Named rather than papered
/// over: it is the same wrapper cell absent everywhere else.
#[rtse::entry]
pub fn sloppy_this(receiver: u64) -> u64 {
    with_current(|context| {
        let nullish = match Value(receiver).kind() {
            crate::value::Kind::Singleton(number) => {
                number == context.singletons.undefined || number == context.singletons.null
            }
            _ => false,
        };
        if !nullish {
            return receiver;
        }
        match holder(context) {
            Some(object) => Value::from_slot(object).bits(),
            None => receiver,
        }
    })
}

/// What a provided name currently holds, by its spelling.
///
/// The same read [`global_get`] performs — the property first, the lazy
/// registration on a miss — for a caller that has a NAME rather than a key
/// number. `eval` is the one: it asks whether the global still holds the
/// function this runtime built, and a native has no compiled key to ask with.
///
/// `None` for a name this runtime does not supply and the program never
/// assigned, which is the same answer a read would give.
pub(in crate::entry) fn provided_value(name: &str) -> Option<u64> {
    with_current(|context| {
        let text = crate::text::Str::from_str(name);
        let key = context.interner.intern(&text, &mut context.keys);
        let object = holder(context)?;
        if let Some(found) = read_property(context, object, Key::Name(key)) {
            return Some(found.bits());
        }
        supply(context, key)
    })
}

/// Materialises a provided global by name, if it is not already there.
///
/// # Why a prototype needs this
///
/// `Array.prototype.constructor` is written by `Array`'s own registration, and
/// the registrations are lazy — so a program that never spells `Array` had
/// `[].constructor === undefined`, where every other runtime answers the
/// constructor. The prototype is what the program reached, and the link back to
/// the constructor is part of the prototype rather than a decoration on the
/// name: the species protocol reads it, `.constructor.name` reads it, and a
/// subclass check reads it.
///
/// Routed through the global OBJECT rather than by calling the registration
/// directly, because a registration builds a fresh callable each time it runs:
/// calling it twice would make `Array !== [].constructor`, which is a worse
/// answer than the missing one.
pub(in crate::entry) fn ensure(context: &mut Context, name: &str) {
    let Some(object) = holder(context) else {
        return;
    };
    let key = context.well_known(name);
    if read_property(context, object, key).is_some() {
        return;
    }
    if let Key::Name(named) = key {
        supply(context, named);
    }
}

/// The global object, as a value a host can hand to a program.
///
/// # Why a host needs this at all
///
/// Because a host installs names *on* it — `declare_global` — but Node also
/// publishes the object itself under a second name (`global`), and that is a
/// fact about Node rather than about a global object. Answering the value here
/// keeps the one object one object: a host minting an ordinary object to stand
/// in for it would make `global === globalThis` false, which is a line real
/// programs write.
pub fn global_object(context: &mut Context) -> u64 {
    match holder(context) {
        Some(cell) => crate::value::Value::from_slot(cell).bits(),
        None => super::objects::undefined_of(context),
    }
}

/// The object the provided names are properties of, made once.
pub(in crate::entry) fn holder(context: &mut Context) -> Option<u32> {
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
        let singletons = Singletons { undefined: 0, null: 1, hole: 2 };
        let context = Context::new(singletons, crate::value::Kinds::in_declaration_order());
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
