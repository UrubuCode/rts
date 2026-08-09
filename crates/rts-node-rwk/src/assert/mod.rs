//! `node:assert` — synchronous invariant checks.
//!
//! # A failed assertion now raises a catchable error
//!
//! It used to be that no native here could throw an error a program catches.
//! [`rts_core_rwk::entry::throw`]'s own module doc has the history: finding a
//! handler one frame up used to mean unwinding the native stack, which needed
//! an exception table this repository did not have; a throw is recorded now
//! and every compiled call site checks, so a value no handler in the
//! throwing function caught leaves that one frame and a `try` above it sees
//! it.
//!
//! What blocked THIS crate was narrower — **visibility**, not mechanism —
//! and it is fixed: `rts_core_rwk::entry::throw_type_error(message)` is
//! public, raises the program's own `TypeError` (so `e instanceof TypeError`
//! holds), and is what [`values::report_failure`] calls. It is not Node's own
//! `AssertionError`: reaching the `AssertionError`/named-error constructors
//! this module builds (`assertion_error` below) from inside a raise would
//! need a public spelling this crate does not have either, so what a program
//! catches here has the operator and both values in `.message` but not
//! Node's `name`/`code`/`actual`/`expected` shape. `rts-std-rwk`'s
//! `globals/events/abort.rs` records the same visibility finding from the
//! other side.
//!
//! Everything below follows from that. A failed assertion prints
//! `AssertionError [operator] #n: <detail>` to **stderr**, increments a
//! process-wide counter, and raises — all three from
//! [`values::report_failure`]. **What a program sees**: the assertion
//! function does not return; the raise unwinds to the nearest `try`/`catch`,
//! or ends the program if there is none, exactly as real `assert` does. A
//! passing assertion is silent, exactly as real `assert` is.
//!
//! # What the reuse search found
//!
//! - `rts_core_rwk::entry` (read in full): `strict_equals` is `===` and
//!   `loose_equals` is `==`; **no** `Object.is`, no deep comparison, no class
//!   identity, no accessor installation. `number_of` is a real number type test
//!   and `string_in`/`is_callable_in` are real type tests, so
//!   [`equality::same_value`] is built from them rather than from a coercion.
//! - `node:util`'s private `deep_equal` — the near miss, named in
//!   [`equality`]'s doc along with why calling it would make this weaker.
//! - `crate::fs::options_and_listener` — the argument-shift helper. Not needed:
//!   no assertion in Node's surface has an optional argument *before* a
//!   required one.
//!
//! # Not implemented, by name
//!
//! - **`throws`, `doesNotThrow`** — both are defined by catching what `fn()`
//!   raises. A throw inside `fn` exits the process before this native resumes,
//!   so there is nothing to catch and nothing to inspect. Not approximable:
//!   answering "it threw" without having seen a throw is a fabricated result.
//! - **`rejects`, `doesNotReject`** — need two things this surface lacks: a
//!   rejection reason read from a promise, and a promise that settles LATER.
//!   `entry::settled` mints an already-settled promise and there is no deferred
//!   form, so the `Promise<void>` these must return cannot be produced honestly.
//! - **`assert.strict` reference identity with `node:assert/strict`.** The
//!   `strict` object exists here and `strict.strict === strict` holds; the
//!   `node:assert/strict` **specifier** is registered in `lib.rs`, which this
//!   module does not own. Until it is, that import resolves to nothing.
//! - **`message` being an `Error` thrown as-is.** Node distinguishes it from a
//!   string by throwing it unwrapped. With no throw the distinction has nowhere
//!   to go, so an `Error` message is rendered like any other value.
//! - **`AssertionError instanceof Error`.** The class here has `name`,
//!   `message`, `code`, `actual`, `expected`, `operator` and
//!   `generatedMessage`, and its own prototype — but the host surface exposes
//!   no way to reach the primordial `Error` to inherit from, so the chain stops
//!   at `AssertionError.prototype`. No `stack` either, for the same reason.
//! - **`Assert`'s `diff: "simple" | "full"`.** Accepted and ignored; the
//!   generated message is this module's own rendering either way, since
//!   `node:util`'s `inspect` is private to that module. `strict` and
//!   `skipPrototype` ARE honored, per-instance.
//! - **`Map` and `Set` deep comparison.** [`equality`]'s `identity_only`
//!   refuses them — their entries are internal slots this surface cannot walk,
//!   so a structural comparison would report every pair of `Map`s equal. The
//!   refusal makes two equal `Map`s report a failure instead, which is loud
//!   rather than silent. Any object carrying both a `has` and a `delete`
//!   method is caught by the same duck test. `Promise`, `WeakMap` and `WeakSet`
//!   fall in it too, and for them identity is Node's own answer.
//! - **Typed-array constructor identity.** An `Int8Array` and a `Uint8Array`
//!   are compared element by element, so identical *element values* compare
//!   equal where Node requires the same concrete constructor. Nothing on this
//!   surface names a value's class.
//! - **Symbol-keyed properties** in deep comparison — `own_keys` answers
//!   enumerable string keys only.
//! - **`Error` `name`/`message`/`cause`/`errors` compared despite being
//!   non-enumerable**, and the boxed-wrapper (`new Number(1)`) rules. Both need
//!   reads this surface does not offer for non-enumerable slots.
//! - **`CallTracker`** — removed from Node itself.

mod checks;
mod equality;
mod values;

use rts_core_rwk::entry::{self, Context, Provided};
use values::member;

/// The namespace `node:assert` is.
///
/// Built as a **callable** rather than a plain object — [`entry::make_callable`]
/// over `ok` — so `assert(x)` works exactly as `assert.ok(x)` does, which is
/// what real Node's default export is. [`entry::put_member`] still works on it:
/// a callable is a cell with a slot like any other.
pub fn namespace(context: &mut Context) -> u64 {
    // Legacy mode at the top level: `assert.equal(1, "1")` passes in Node, and
    // the strict view below is the other half.
    let callable = view(context, false, false);
    let strict = view(context, true, false);
    // `require("node:assert").strict.strict` is documented to keep working, so
    // the strict view names itself.
    entry::put_member(context, strict, "strict", strict);
    entry::put_member(context, callable, "strict", strict);
    for holder in [callable, strict] {
        let error_class = entry::make_callable(context, assertion_error);
        entry::put_member(context, holder, "AssertionError", error_class);
        let assert_class = entry::make_callable(context, assert_class);
        entry::put_member(context, holder, "Assert", assert_class);
    }
    callable
}

/// One assertion object: callable as `ok`, carrying every member.
///
/// The two module-level views (`assert` and `assert.strict`) and every `Assert`
/// instance are the same construction with different flags, so there is one
/// place that decides which function a name means.
fn view(context: &mut Context, strict: bool, skip_prototype: bool) -> u64 {
    let object = entry::make_callable(context, checks::ok);
    install(context, object, strict, skip_prototype);
    object
}

/// Hangs the member table on an object.
fn install(context: &mut Context, object: u64, strict: bool, skip_prototype: bool) {
    for (name, code) in table(strict, skip_prototype) {
        let value = entry::make_callable(context, code);
        entry::put_member(context, object, name, value);
    }
}

/// Which native each name means, under one configuration.
///
/// # Why the choice is made here and not inside the natives
///
/// A native carries no state, so a member that behaves differently per object
/// has to BE a different function. That is what preserves Node's documented
/// caveat for free: `const { equal } = new Assert({ strict: false })` keeps the
/// loose comparison, because the loose comparison is the function that was
/// destructured — not a shared method that would have read a receiver it no
/// longer has.
fn table(strict: bool, skip_prototype: bool) -> Vec<(&'static str, Provided)> {
    let (deep, not_deep): (Provided, Provided) = match skip_prototype {
        true => (checks::deep_strict_open, checks::not_deep_strict_open),
        false => (checks::deep_strict_equal, checks::not_deep_strict_equal),
    };
    let (equal, not_equal): (Provided, Provided) = match strict {
        true => (checks::strict_equal, checks::not_strict_equal),
        false => (checks::equal, checks::not_equal),
    };
    let (deep_equal, not_deep_equal): (Provided, Provided) = match strict {
        true => (deep, not_deep),
        false => (checks::deep_equal_loose, checks::not_deep_equal_loose),
    };
    vec![
        ("ok", checks::ok as Provided),
        ("equal", equal),
        ("notEqual", not_equal),
        ("strictEqual", checks::strict_equal),
        ("notStrictEqual", checks::not_strict_equal),
        ("deepEqual", deep_equal),
        ("notDeepEqual", not_deep_equal),
        ("deepStrictEqual", deep),
        ("notDeepStrictEqual", not_deep),
        ("partialDeepStrictEqual", checks::partial_deep_strict_equal),
        ("match", checks::matches),
        ("doesNotMatch", checks::does_not_match),
        ("fail", checks::fail),
        ("ifError", checks::if_error),
    ]
}

/// `new Assert(options?)`.
///
/// The configuration is read BEFORE the borrow is opened: every read of an
/// options field is an ambient entry point, and calling one from inside
/// [`entry::with_runtime`] is a second borrow of the same `RefCell` — a panic
/// an `extern "C"` frame cannot unwind past, which aborts the process.
extern "C" fn assert_class(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let strict = flag(options, "strict", true);
    let skip_prototype = flag(options, "skipPrototype", false);
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Assert", &[]);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        install(context, instance, strict, skip_prototype);
        instance
    })
}

/// `new assert.AssertionError(options)`.
///
/// Constructed rather than thrown — see the module doc. It exists so a program
/// that builds one (a test framework reporting its own failure in Node's shape)
/// finds the properties it expects, and so the operator names this module
/// prints have a class to belong to.
extern "C" fn assertion_error(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let (actual, expected) = match is_object(options) {
        true => (member(options, "actual"), member(options, "expected")),
        false => (absent, absent),
    };
    let operator = match is_object(options) {
        true => values::string_of(member(options, "operator")).unwrap_or_default(),
        false => String::new(),
    };
    // A string message is the caller's verbatim and sets `generatedMessage`
    // false; anything else — absent included — means this built it. Asked with
    // `string_of`, which is a type test: a coercion would report a number
    // message as caller-supplied text and set the flag the wrong way.
    let supplied = match is_object(options) {
        true => values::string_of(member(options, "message")),
        false => None,
    };
    let generated = supplied.is_none();
    let message = supplied.unwrap_or_else(|| {
        format!(
            "Expected values to satisfy {operator}:\n  actual:   {}\n  expected: {}",
            values::render(actual),
            values::render(expected)
        )
    });
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "AssertionError", &[]);
        let instance = match entry::is_object(context, this) {
            true => this,
            false => entry::make_instance(context, prototype),
        };
        let message = entry::make_string(context, &message);
        entry::put_member(context, instance, "message", message);
        let name = entry::make_string(context, "AssertionError");
        entry::put_member(context, instance, "name", name);
        let code = entry::make_string(context, "ERR_ASSERTION");
        entry::put_member(context, instance, "code", code);
        let operator = entry::make_string(context, &operator);
        entry::put_member(context, instance, "operator", operator);
        entry::put_member(context, instance, "actual", actual);
        entry::put_member(context, instance, "expected", expected);
        let generated = entry::boolean_value(generated);
        entry::put_member(context, instance, "generatedMessage", generated);
        instance
    })
}

/// One boolean option, defaulting when the whole options object is absent or
/// the field is not there.
fn flag(options: u64, name: &str, default: bool) -> bool {
    if !is_object(options) {
        return default;
    }
    let held = member(options, name);
    match held == entry::undefined_value() {
        true => default,
        false => entry::to_boolean(held),
    }
}

/// Whether a value is something a member can be read off at all — asked before
/// every options read, so `new Assert()` with no argument does not index
/// `undefined`.
fn is_object(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_object(context, value))
}
