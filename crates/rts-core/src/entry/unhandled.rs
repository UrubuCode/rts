//! What `emitter.emit('error')` owes when nothing is listening.
//!
//! # Why this is its own file and not three functions in [`super::errors`]
//!
//! That module is the obvious home — it is the `ERR_*` raiser every surface
//! shares — and it was 433 lines before this change. These three functions are
//! 117 of them, which puts it at 550 and over this workspace's 500-line
//! ceiling. Splitting `errors` into a folder would move nine unrelated raisers
//! to make room for one; a small module named after the single moment it serves
//! does not. [`super::errors::constructor_name_in`] is shared rather than
//! copied, which is the one thing this file reaches back for.
//!
//! # Why it is in this crate rather than in either emitter
//!
//! There are two `EventEmitter`s in this workspace — `rts-node::events` behind
//! `node:events` and `rts-std::globals::emitter` as the bare global — and both
//! own this same moment. `rts-std` cannot name `rts-node`, so the alternative
//! was the same raiser written twice, which is exactly the drift
//! [`super::errors`]' header argues against for `ERR_INVALID_ARG_TYPE`: a
//! program reads the CODE, and two spellings fail a test that says nothing
//! about the module under test. Both crates already depend on this one.
//!
//! Rejected: `rts-node::errors::raise`, which stamps `code` but has no way to
//! put the raw value on `.context`, and is `pub(crate)` to a crate `rts-std`
//! must not depend on.
//!
//! # Why it raises at all, where it used to exit
//!
//! Both emitters printed a diagnostic and called `std::process::exit(1)` from
//! inside the native, on the stated belief that a native cannot raise a value
//! compiled code could catch. That belief is out of date: `rts-core`'s rule 8
//! is the discipline that makes raising safe, and `throw::throw_value` is the
//! raise. The cost of the old behaviour was measured rather than argued —
//! `@whiskeysockets/baileys` emits an `'error'` inside `makeWASocket`'s own
//! assembly and expects to catch it, so the process died before the
//! constructor returned.

use super::with_current;

/// Raises what `emitter.emit('error')` owes when nothing is listening.
///
/// An `Error` argument is raised VERBATIM — the identity matters, because
/// `catch (e) { e === theErrorIEmitted }` is true in Node and a rebuilt copy
/// would answer false. Anything else is wrapped in a plain `Error` carrying
/// `code === "ERR_UNHANDLED_ERROR"` and the original value on `.context`, which
/// is Node's own shape, measured against Node 22 before this was written.
pub fn unhandled_error(value: u64) {
    if is_error(value) {
        super::throw::throw_value(value);
        return;
    }
    let message = format!("Unhandled error. ({})", inspected(value));
    let Some(error) = super::throw::make_named_error("Error", &message) else {
        // No primordials yet, so nothing to build from. Raising the plain type
        // error still STOPS the emit, which is the whole point — swallowing it
        // is the behaviour this function exists to remove.
        super::throw::type_error(&message);
        return;
    };
    with_current(|context| {
        let held = super::modules::make_string(context, "ERR_UNHANDLED_ERROR");
        super::modules::put_member(context, error, "code", held);
        super::modules::put_member(context, error, "context", value);
    });
    super::throw::throw_value(error);
}

/// Whether `value`'s prototype chain reaches `Error.prototype`.
///
/// The chain and not a brand: [`super::functions::instance_of`] wants the global
/// `Error` CONSTRUCTOR, and [`super::host_class::error_prototype`]'s own doc
/// records why reading that off the global object answers nothing — the error
/// family lives in this crate's class table rather than as a property there. So
/// the question is asked of the prototype that table hands back, which is the
/// same object every `new Error` is linked to.
///
/// The step count is bounded because a program can build a cyclic chain with
/// `Object.setPrototypeOf`, and this runs on a path that is already failing: a
/// hang here would replace a reported error with silence.
fn is_error(value: u64) -> bool {
    let Some(prototype) = with_current(super::host_class::error_prototype) else {
        return false;
    };
    let (null, undefined) = with_current(|context| {
        (super::modules::null_in(context), super::modules::undefined_in(context))
    });
    let mut walking = value;
    for _ in 0..64 {
        let next = super::chain::get_prototype(walking);
        if next == prototype {
            return true;
        }
        if next == walking || next == null || next == undefined {
            return false;
        }
        walking = next;
    }
    false
}

/// A value as Node's `Unhandled error. (…)` message spells it.
///
/// Not `errors::kind_text`, which answers the same dispatch in
/// `ERR_INVALID_ARG_TYPE`'s words (*"type string ('x')"*) — two messages, two
/// questions, and collapsing them would mean a mode flag at every call site.
///
/// # The divergence, stated
///
/// Node renders a non-primitive through `util.inspect`, so `{ a: 1 }` prints as
/// `{ a: 1 }`. That renderer is not reachable from here (it lives above this
/// crate) and [`super::text::described`] is the wrong substitute — it runs the
/// value's own `toString`, which is user code on a path that is already raising.
/// So an object is named by its constructor instead: `[Object]`. The `code` and
/// the `.context` a program branches on are exact either way.
fn inspected(value: u64) -> String {
    let (undefined, null, text) = with_current(|context| {
        (
            super::modules::undefined_in(context),
            super::modules::null_in(context),
            super::modules::string_in(context, value),
        )
    });
    if value == undefined {
        return String::from("undefined");
    }
    if value == null {
        return String::from("null");
    }
    if let Some(text) = text {
        return format!("'{text}'");
    }
    let heap_object = crate::value::Value(value).as_slot().is_some()
        && super::modules::number_of(value).is_none();
    if heap_object {
        let name = with_current(|context| super::errors::constructor_name_in(context, value));
        return format!("[{}]", name.unwrap_or_else(|| String::from("Object")));
    }
    super::text::described(value).unwrap_or_else(|| String::from("undefined"))
}
