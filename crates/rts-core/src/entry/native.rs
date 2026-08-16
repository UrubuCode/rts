//! A function whose code is Rust, reached the way any other function is.
//!
//! # Why this is not a second kind of callee
//!
//! Every compiled JavaScript function has one shape —
//! `extern "C" fn(env, this, a0..a3) -> value` — and a Rust function can have
//! that shape. So a built-in method is an ordinary callable whose code address
//! happens to point at Rust, and [`super::functions::call`] never finds out.
//!
//! The alternative is teaching `call` about a second kind: a tag beside the
//! cell, a branch, and a second dispatch. That puts a test on **every call in
//! the program** to serve the few that are built in, and it does it in the one
//! function this engine most wants to keep straight.
//!
//! # Why the environment is `undefined`
//!
//! A native closes over nothing. The slot exists because every callable has one,
//! not because these use it — and passing `undefined` rather than leaving it
//! uninitialised is what makes that legible at the call.
//!
//! # Why this module exists at all
//!
//! It was written inside the regular-expression module, where `test` and `exec`
//! needed it first. The string prototype needs the same three things —
//! make a callable, name it, hang it on an object — and a second copy of "how a
//! built-in is installed" is the rule written twice this crate keeps refusing.

use super::Context;
use super::objects::undefined_of;
use crate::text::Str;
use crate::value::Value;

/// The shape a compiled function has, which a Rust one can also have.
///
/// Spelled once. Two spellings of a calling convention is how an argument comes
/// to be read as the wrong thing, and a wrong one is a jump with a corrupt
/// stack rather than a wrong answer.
pub(in crate::entry) type Native = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// A callable value over a Rust function.
pub(super) fn callable(context: &mut Context, code: Native) -> u64 {
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
    let environment = undefined_of(context);
    context.mark_callable(cell, code as usize as u64, environment);
    Value::from_slot(cell).bits()
}

/// Hangs a set of them on an object, by name.
///
/// The object is what a value inherits from, so this is what makes `s.trim` and
/// `re.test` findable by the ordinary prototype walk rather than by anything
/// knowing what a string or a regular expression is.
pub(in crate::entry) fn install(context: &mut Context, cell: u32, natives: &[(&str, Native)]) {
    for (name, code) in natives {
        let method = callable(context, *code);
        name_of(context, method, name);
        let key = context.well_known(name);
        super::objects::put(context, cell, key, method);
        hidden(context, cell, key);
    }
}

/// Marks a member NON-ENUMERABLE, which every built-in method is.
///
/// It went unnoticed for as long as `for`-`in` walked own keys only: nothing
/// enumerated a prototype. The moment it walked the chain,
/// `for (const k in {})` answered `hasOwnProperty,isPrototypeOf,…` — every
/// method the engine installs — which is a program-visible difference on the
/// most ordinary loop there is.
///
/// Written here rather than at each `NATIVES` table because the rule has no
/// exception: the specification gives every built-in method
/// `{ writable: true, enumerable: false, configurable: true }`, so a table that
/// could opt out would only ever be opting into a bug.
pub(in crate::entry) fn hidden(context: &mut Context, cell: u32, key: crate::object::Key) {
    if let crate::object::Key::Name(named) = key {
        super::integrity::set_attributes(context, cell, named, super::integrity::Attributes {
            writable: true,
            enumerable: false,
            configurable: true,
        });
    }
}

/// Like [`install`], for a list whose entries also declare `.length` — the
/// spec's arity, which the specification requires on every named function.
///
/// A second list shape rather than a length on every one of this crate's
/// dozens of `NATIVES` tables: most of those are called through property
/// access on their receiver and a program almost never reads the function
/// value itself, so most callers pay nothing. This one exists because
/// `Object.assign`, `Object.keys` and friends ARE read as values — a program
/// forwards them, wraps them, or introspects them — and the specification
/// pins an exact arity for each.
pub(in crate::entry) fn install_with_arity(context: &mut Context, cell: u32, natives: &[(&str, Native, u32)]) {
    for (name, code, arity) in natives {
        let method = callable(context, *code);
        name_of(context, method, name);
        length_of(context, method, *arity);
        let key = context.well_known(name);
        super::objects::put(context, cell, key, method);
        hidden(context, cell, key);
    }
}

/// Writes `.name` on a callable — a real property, per the specification's own
/// `SetFunctionName`, so `fn.name` reads the same whether `fn` is a built-in or
/// a declared one.
pub(in crate::entry) fn name_of(context: &mut Context, callable: u64, name: &str) {
    if let Some(cell) = Value(callable).as_slot() {
        let key = context.well_known("name");
        let value = context.intern_value(Str::from_str(name)).bits();
        super::objects::put(context, cell, key, value);
        introspective(context, cell, key);
    }
}

/// Writes `.length` on a callable — the parameter count the specification's
/// `SetFunctionLength` puts there before a body ever runs.
fn length_of(context: &mut Context, callable: u64, arity: u32) {
    if let Some(cell) = Value(callable).as_slot() {
        let key = context.well_known("length");
        let value = Value::from_f64(f64::from(arity)).bits();
        super::objects::put(context, cell, key, value);
        introspective(context, cell, key);
    }
}

/// Marks `name` or `length` on a callable: **non-writable**, non-enumerable,
/// configurable.
///
/// Not [`hidden`], which is the attribute set for a METHOD — writable, so that a
/// program may replace `Array.prototype.map`. `SetFunctionName` and
/// `SetFunctionLength` both spell out `[[Writable]]: false` instead, and the
/// difference is program-visible twice: `Object.keys(Array.prototype.map)`
/// answered `["name"]` because nothing marked it non-enumerable, and
/// `fn.name = "x"` stored a new name where the language refuses the write and
/// leaves `defineProperty` as the only way through.
fn introspective(context: &mut Context, cell: u32, key: crate::object::Key) {
    if let crate::object::Key::Name(named) = key {
        super::integrity::set_attributes(context, cell, named, super::integrity::Attributes {
            writable: false,
            enumerable: false,
            configurable: true,
        });
    }
}

/// Hangs a native **getter** on an object, by name.
///
/// # Why this exists beside [`install`]
///
/// Because a getter is not a method, and installing it as one is a wrong answer
/// that reads correctly. `Map.prototype.size`, `RegExp.prototype.flags` and
/// `Symbol.prototype.description` are accessors in the language, and this engine
/// served each of them as something else — a data property on the INSTANCE, or
/// nothing at all. Both work for `m.size`, and both fail the moment a program
/// asks about the property rather than through it:
/// `Object.getOwnPropertyDescriptor(Map.prototype, "size")` answered `undefined`
/// for a property every Map has, which is two readable facts about one object
/// that cannot both be true. Thirty-seven fixtures died on exactly that
/// `undefined`, reading `.get`, `.writable` or `.value` off it.
///
/// The instance-data-property spelling has a second cost the descriptor hides:
/// `m.size = 5` STORED, and the next mutation overwrote it. An accessor with no
/// setter refuses the write instead, which is what the language says.
///
/// # Why the pair is `(get, None)` and not `(get, get)`
///
/// A setter-less accessor is the point. The specification gives every one of
/// these `[[Set]]: undefined`, so assigning is refused — and a `set` that
/// silently did nothing would look identical from inside the engine while
/// answering `true` to `Reflect.set`, which is the observable difference.
pub(in crate::entry) fn getter(context: &mut Context, cell: u32, name: &str, code: Native) {
    let function = callable(context, code);
    // `get size`, which is what `SetFunctionName` puts on an accessor's getter —
    // a program reading `descriptor.get.name` sees the prefix, and it is the
    // only place the two halves of an accessor pair are told apart by name.
    name_of(context, function, &format!("get {name}"));
    let key = context.well_known(name);
    if let crate::object::Key::Name(named) = key {
        context.define_accessor_and_invalidate(cell, named.index() as u32, Some(function), None);
        // Non-enumerable and configurable, like every built-in member. Recorded
        // rather than assumed: `super::integrity::effective` is what the
        // descriptor reads, and a property it has no record for reports the
        // defaults, which say enumerable.
        super::integrity::set_attributes(context, cell, named, super::integrity::Attributes {
            writable: false,
            enumerable: false,
            configurable: true,
        });
    }
}

/// An object with nothing on it, for something to be a prototype.
pub(in crate::entry) fn plain(context: &mut Context) -> Option<u32> {
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    super::alloc::alloc_after_collecting(context, crate::heap::STRIDE, ty)
}
