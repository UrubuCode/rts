//! The names the runtime provides, and how one is read.
//!
//! # What a name here is, and why it is a call
//!
//! `RegExp` is not a constant the way `NaN` is. It is an object with a
//! `prototype`, allocated once, that a program can write properties to — so the
//! emitter cannot produce it, and a call is what reaches the one the runtime
//! made. The number that crosses is the key the compiler already resolved.
//!
//! # The one thing kept stricter than the language
//!
//! A name that is neither provided here nor assigned anywhere in the program is
//! [`super::EmitError::UnboundName`] rather than a read answering `undefined`.
//! The language throws a `ReferenceError` there, which this engine cannot raise
//! where a handler could catch it — so the choice is between a refusal that is
//! wrong for a program meaning to CATCH that error and an `undefined` that is
//! wrong for every program with a typo in it. `typeof` is exempt, as the
//! specification exempts it, and that exemption is implemented rather than
//! approximated.
//!
//! # Why the list lives in this crate
//!
//! Because which names the global object has is a fact about **JavaScript** —
//! ECMA-262 §19 enumerates them — and this crate is the one that knows the
//! language. The runtime decides what it can supply, which is a different
//! question, and answers `undefined` for a name it does not have.
//!
//! That asymmetry is deliberate rather than sloppy: a name listed here and
//! missing there is a value a program can see, where the alternative — the
//! runtime naming the set — would make the compiler ask permission from
//! whichever runtime it happened to be built against, which is the boundary
//! rule 1 draws.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr::call;
use super::property::key_constant;
use super::{Ctx, EmitResult};
use crate::names::Name;
use crate::runtime::RuntimeOp;

/// The names this engine supplies as values rather than as constants.
///
/// Short on purpose. Every entry is a name a program may read without declaring
/// it, so a name added here stops being a `ReferenceError` — which is a language
/// decision and not a convenience.
///
/// `globalThis` is the object itself, which is what makes this a global object
/// rather than a table with globals in it: a program can reach it, enumerate it,
/// and put something on it.
/// `console` is here for the same reason and is worth its own sentence: it is
/// in no specification, and every runtime has it. A program writes
/// `console.log` with no import line, so refusing the NAME would refuse the
/// program — and which of these actually has a value is the runtime's to answer,
/// which is why this list may be longer than what any one host installs.
const PROVIDED: &[&str] = &[
    "Array",
    "console",
    "ArrayBuffer",
    "BigInt",
    "BigInt64Array",
    "BigUint64Array",
    "Boolean",
    "Buffer",
    "Date",
    "DataView",
    "Error",
    "EvalError",
    "Float32Array",
    "Float64Array",
    "Function",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "JSON",
    "Map",
    "Math",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
    "Number",
    "Object",
    "Promise",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Set",
    "String",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "structuredClone",
    "Symbol",
    "SyntaxError",
    "TypeError",
    "URIError",
    "WeakMap",
    "WeakSet",
    "globalThis",
    // The rest of this list is in no ECMA-262 section, and belongs here for the
    // reason `console`'s own sentence gives: a program writes them with no
    // import line, so refusing the NAME refuses the program. Whether any of them
    // has a value is the runtime's answer and not this crate's — which is the
    // asymmetry stated above, and the reason this list may be longer than what a
    // given host installs.
    //
    // Measured rather than guessed: these are the names the suite's own files
    // reach for undeclared, ranked by how many files each one refused
    // (`rts-host-rwk`'s `suite_coverage`). `URL` and `TextEncoder` cost six
    // files each, `URLSearchParams` three.
    //
    // **Every name below has a real value in this workspace's host.** That is a
    // rule and not an accident: a name listed here whose value does not exist
    // turns a refusal at compile time into an `undefined` at run time, and it
    // makes the compile-rate measurement count a file that cannot work. This
    // repository's honesty floor names that exact failure — a number measured
    // against a corpus quietly smaller than claimed. `fetch`, `Blob`, `require`,
    // `Proxy`, `AggregateError`, `SharedArrayBuffer`, `WeakRef`,
    // `FinalizationRegistry`, `Atomics`, `eval` and `Intl` are all reached by
    // the suite and are all ABSENT here for that reason, until something
    // supplies them. The WHATWG group below was added by the change that
    // installed it in `rts-std-rwk`, not before.
    //
    // Node's:
    "process",
    "setTimeout",
    "clearTimeout",
    "setInterval",
    "clearInterval",
    "setImmediate",
    "clearImmediate",
    // WHATWG's, which every JavaScript host outside a browser also provides:
    "URL",
    "URLSearchParams",
    "performance",
    // This engine's own, and the reason they are on this list rather than in a
    // module: `globals/output.rs` measured that the module shape was not being
    // used, and a program declaring its own `print` shadows this one anyway.
    "print",
    "println",
    "prompt",
    "TextEncoder",
    "TextDecoder",
    "atob",
    "btoa",
    "AbortController",
    "AbortSignal",
    "Event",
    "EventTarget",
    "CustomEvent",
];

/// Whether a name resolves against the global object.
///
/// Two ways to qualify, and they are different facts. A **provided** name is one
/// the language says exists. A **created** one is a name this program assigns to
/// without declaring, which is how sloppy mode makes a global — and it is
/// answered from a scan of the whole program because the read can be emitted
/// before the assignment is reached.
pub(super) fn resolves(ctx: &Ctx, name: Name) -> bool {
    PROVIDED.contains(&ctx.names.text(name)) || ctx.globals.contains(&name)
}

/// Emits a read of one, if it is one.
///
/// `None` means the name is neither provided nor created, which the caller turns
/// back into the unbound-name refusal rather than into `undefined`. That
/// refusal is **stricter** than the language, which throws a `ReferenceError`
/// this engine cannot raise where a handler could catch it — and it is wrong
/// only for a program that meant to catch that error, where answering
/// `undefined` would be wrong for every program with a typo in it.
pub(super) fn read(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    name: Name,
) -> Option<EmitResult<ValueId>> {
    if !resolves(ctx, name) {
        return None;
    }
    Some(force_read(builder, ctx, name))
}

/// The same read, for a caller that has already decided the name is global.
///
/// `typeof` uses it. The specification exempts `typeof` from the
/// `ReferenceError` an undeclared read raises — it takes a reference rather
/// than a value — so a name nothing declared has to reach the global object
/// there even when it would be refused anywhere else.
pub(super) fn force_read(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    name: Name,
) -> EmitResult<ValueId> {
    // By key, not by index into the list above: the runtime holds these as
    // properties of an object, so the number that crosses is the one the key
    // registry issued — the same numbering every other property read uses. A
    // position in this list would be a second numbering for the same names.
    let key = key_constant(builder, ctx, name);
    Ok(call(builder, ctx, RuntimeOp::GlobalGet, &[key])?[0])
}

/// Emits a write, creating the global.
///
/// The whole of sloppy mode's global creation: an assignment to a name nothing
/// declared puts a property on the global object. Strict mode throws instead,
/// which is the same `ReferenceError` this engine cannot raise — so the sloppy
/// answer is implemented and the strict one is a stated gap rather than a
/// silently different language.
pub(super) fn write(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    name: Name,
    value: ValueId,
) -> EmitResult<ValueId> {
    let key = key_constant(builder, ctx, name);
    Ok(call(builder, ctx, RuntimeOp::GlobalSet, &[key, value])?[0])
}
