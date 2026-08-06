//! What an `import` binds to, when the module is one the host provides.
//!
//! # What this is NOT
//!
//! A module system. Nothing here reads a file, resolves a path, orders an
//! evaluation, or links a cycle. Those are real and they belong above this
//! crate: reading a file is the host's, and deciding what a specifier means is
//! the language's.
//!
//! What this is: a table from a specifier to an **object**, and a read of one of
//! its properties. That is exactly enough for `import { test } from "rts:test"`,
//! which is what every file in this repository's own suite begins with, and it
//! is deliberately not enough for `import { x } from "./other.ts"` — which
//! answers `undefined` rather than pretending, and is the gap a real module
//! system fills.
//!
//! # Why an object rather than a table of names
//!
//! The reason [`super::global`] records for the global object: a namespace IS an
//! object in the language — `import * as ns` binds one, and `ns.test` is an
//! ordinary property read. A table keyed by name would answer the read and have
//! nothing to say about the namespace itself.
//!
//! And it means the number that crosses for the imported NAME is a key from the
//! registry the compiler mints from, rather than a second numbering over the
//! same names.
//!
//! # Why the specifier crosses as a literal number
//!
//! Because it is a string the compiler already has: `"rts:test"` is written in
//! the program, so it is in the literal table like every other string, and
//! handing over the text again at every import would hand over something already
//! resolved. The same decision `string_const` records.

use super::objects::{read_property, undefined_of};
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

impl Context {
    /// The namespace object a specifier names, if the host provided one.
    fn module_at(&self, specifier: &str) -> Option<u64> {
        self.modules
            .iter()
            .find(|(name, _)| name == specifier)
            .map(|(_, object)| *object)
    }
}

/// Registers a module the host provides, by specifier.
///
/// A linear list rather than a map: a host provides a handful of these, and the
/// same reasoning the accessor table records applies — hashing a specifier costs
/// more than walking five of them.
pub fn declare_module(context: &mut Context, specifier: &str, namespace: u64) {
    let held = context
        .modules
        .iter_mut()
        .find(|(name, _)| name == specifier);
    match held {
        Some((_, object)) => *object = namespace,
        None => context.modules.push((specifier.to_owned(), namespace)),
    }
}

/// One name imported from one module.
///
/// # Why this is a read rather than a binding
///
/// A live binding — where the exporting module reassigning `x` is seen by the
/// importer — needs the two sides to share a cell, and nothing here has two
/// sides: a host module is finished before the program starts. So an import
/// reads the namespace once, at the point the program reaches it, and that is
/// the divergence to state rather than the mechanism to fake.
///
/// `undefined` for a specifier the host did not provide, which is what makes
/// `import { x } from "./other.ts"` a program that runs and finds nothing
/// instead of one that silently reads someone else's `x`.
#[rtse::entry]
pub fn module_binding(specifier: i64, key: i64) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(text) = context
            .literals
            .get(specifier as usize)
            .copied()
            .and_then(|value| Value(value).as_slot())
            .and_then(|cell| context.text_at(cell))
            .and_then(Str::to_rust)
        else {
            return absent;
        };
        let Some(namespace) = context.module_at(&text) else {
            return absent;
        };
        let Some(cell) = Value(namespace).as_slot() else {
            return absent;
        };
        let Ok(number) = u32::try_from(key) else {
            return absent;
        };
        let Some(key) = context.keys.key(number) else {
            return absent;
        };
        read_property(context, cell, crate::object::Key::Name(key)).map_or(absent, |found| found.bits())
    })
}

/// The whole namespace, for `import * as ns from "m"`.
///
/// The same lookup stopping one step earlier, which is why it is here rather
/// than a second function that could come to disagree about what a specifier
/// resolves to.
#[rtse::entry]
pub fn module_namespace(specifier: i64) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        context
            .literals
            .get(specifier as usize)
            .copied()
            .and_then(|value| Value(value).as_slot())
            .and_then(|cell| context.text_at(cell))
            .and_then(Str::to_rust)
            .and_then(|text| context.module_at(&text))
            .unwrap_or(absent)
    })
}

/// The shape a host-provided function must have.
///
/// The same one every native here has and the same one compiled code has —
/// stated once, in [`super::native`], and re-exported rather than re-spelled:
/// two spellings of a calling convention is how an argument comes to be read as
/// the wrong thing, and a wrong one is a jump with a corrupt stack rather than a
/// wrong answer.
pub type Provided = super::native::Native;

/// Builds a namespace object out of Rust functions.
///
/// # Why a host needs this and could not write it
///
/// Making a callable means allocating a cell in the region, recording a code
/// address beside it where no program can reach it, and interning each name into
/// the key registry the compiler mints from. All three are this crate's, and a
/// host reproducing any of them would be reproducing exactly the agreements
/// `rts-host-rwk` exists to hold rather than restate.
///
/// So the host says WHAT is in a module — which is its business, since
/// availability is what decides membership — and this says how one is built.
pub fn make_namespace(context: &mut Context, members: &[(&str, Provided)]) -> u64 {
    let Some(cell) = super::native::plain(context) else {
        return undefined_of(context);
    };
    super::native::install(context, cell, members);
    Value::from_slot(cell).bits()
}

/// The same, for a namespace that also holds already-built values.
///
/// `rts`'s `io` is an object of functions rather than a function, so a namespace
/// has to be able to hold one — and building it is the caller's, because what is
/// inside it is the caller's.
pub fn put_member(context: &mut Context, namespace: u64, name: &str, value: u64) {
    if let Some(cell) = Value(namespace).as_slot() {
        let key = context.well_known(name);
        super::objects::put(context, cell, key, value);
    }
}

/// One member of a namespace, by name.
///
/// The read half of [`put_member`], and it exists for the same reason: interning
/// the name reaches the key registry, which is this crate's.
pub fn get_member(context: &mut Context, object: u64, name: &str) -> u64 {
    let absent = undefined_of(context);
    let Some(cell) = Value(object).as_slot() else {
        return absent;
    };
    let key = context.well_known(name);
    read_property(context, cell, key).map_or(absent, |found| found.bits())
}

/// One Rust function as a callable value.
///
/// The piece [`make_namespace`] is built out of, exported because a caller
/// building an object of methods one at a time — which is what an `expect(x)`
/// is — would otherwise have to build a namespace and read it back.
pub fn make_callable(context: &mut Context, code: Provided) -> u64 {
    super::native::callable(context, code)
}

/// Runs a body with the installed context.
///
/// The public half of the borrow discipline this crate is written around: a
/// caller outside it cannot reach the thread-local, and the rule that a native
/// must not call user code while holding this borrow applies to it exactly as it
/// applies here. See [`super::native`] for what happens when it is broken — an
/// `extern "C"` frame cannot unwind, so it aborts the process.
pub fn with_runtime<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    with_current(body)
}

/// `undefined`, from outside a borrow.
pub fn undefined_value() -> u64 {
    with_current(|context| undefined_of(context))
}

/// `null`, from outside a borrow.
///
/// Not the same as `undefined` and not interchangeable with it: the two are
/// distinct singletons and a matcher comparing against the wrong one would pass
/// for the wrong value.
pub fn null_value() -> u64 {
    with_current(|context| {
        rts_cranelift::tags::encode(
            rts_cranelift::tags::TAG_SINGLETON,
            u64::from(context.singletons.null),
        )
    })
}
