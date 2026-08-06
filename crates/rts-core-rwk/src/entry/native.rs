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
use crate::value::Value;

/// The shape a compiled function has, which a Rust one can also have.
///
/// Spelled once. Two spellings of a calling convention is how an argument comes
/// to be read as the wrong thing, and a wrong one is a jump with a corrupt
/// stack rather than a wrong answer.
pub(super) type Native = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// A callable value over a Rust function.
pub(super) fn callable(context: &mut Context, code: Native) -> u64 {
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    match context.region.alloc(crate::heap::STRIDE, ty) {
        Some(cell) => {
            let environment = undefined_of(context);
            context.mark_callable(cell, code as usize as u64, environment);
            Value::from_slot(cell).bits()
        }
        // The region is full — see [`super::alloc::heap_exhausted`].
        None => super::alloc::heap_exhausted(context),
    }
}

/// Hangs a set of them on an object, by name.
///
/// The object is what a value inherits from, so this is what makes `s.trim` and
/// `re.test` findable by the ordinary prototype walk rather than by anything
/// knowing what a string or a regular expression is.
pub(super) fn install(context: &mut Context, cell: u32, natives: &[(&str, Native)]) {
    for (name, code) in natives {
        let method = callable(context, *code);
        let key = context.well_known(name);
        super::objects::put(context, cell, key, method);
    }
}

/// An object with nothing on it, for something to be a prototype.
pub(super) fn plain(context: &mut Context) -> Option<u32> {
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    context.region.alloc(crate::heap::STRIDE, ty)
}
