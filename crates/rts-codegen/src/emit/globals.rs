//! The names the runtime provides, and how one is read.
//!
//! # What a name here is, and why it is a call
//!
//! `RegExp` is not a constant the way `NaN` is. It is an object with a
//! `prototype`, allocated once, that a program can write properties to — so the
//! emitter cannot produce it, and a call is what reaches the one the runtime
//! made. The number that crosses is the key the compiler already resolved.
//!
//! # A name neither provided here nor assigned anywhere
//!
//! Used to be [`super::EmitError::UnboundName`] unconditionally — refused before
//! the program ran at all, which was **stricter than the language**: JavaScript
//! throws a catchable `ReferenceError` only when such a read actually executes,
//! and a name mentioned only on a dead branch, or reached only through
//! `globalThis` rather than a bare declaration, compiled to nothing under the
//! old rule even though the language runs it. [`unbound_read`] is the runtime
//! side of the real rule now: a name the scope walk, [`super::binding::
//! predefined`] and [`resolves`] all failed to place reaches it, and it always
//! raises. `typeof` is exempt, as the specification exempts it, and that
//! exemption is implemented rather than approximated — see `emit::unary`.
//!
//! This does not cost the compile-time catch a typo'd LOCAL used to get: a name
//! any of those three could place — a declared binding, one of the three
//! emitter constants, a provided or sloppily-created global — never reaches
//! [`unbound_read`] at all. What reaches it is exactly the set the language
//! itself calls unbound, which is why raising there is the correct answer and
//! not a widened one.
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
    "Intl",
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
    "Proxy",
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
    "AggregateError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    "SharedArrayBuffer",
    "Atomics",
    "Iterator",
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
    // (`rts-host`'s `suite_coverage`). `URL` and `TextEncoder` cost six
    // files each, `URLSearchParams` three.
    //
    // **Every name below has a real value in this workspace's host.** That is a
    // rule and not an accident: a name listed here whose value does not exist
    // turns a refusal at compile time into an `undefined` at run time, and it
    // makes the compile-rate measurement count a file that cannot work. This
    // repository's honesty floor names that exact failure — a number measured
    // against a corpus quietly smaller than claimed. `fetch`, `Blob`, `require`,
    // `FinalizationRegistry`, `eval` and `Intl` are all reached by the suite and
    // are all ABSENT here for that reason, until something supplies them.
    // `SharedArrayBuffer`, `WeakRef`, `Atomics`, `EventEmitter`, `Iterator` and
    // `Storage` joined the list below once `rts-core`/`rts-std` grew real
    // values for them — `Atomics` single-threaded and honest about it,
    // `SharedArrayBuffer` with nothing to share WITH for the same reason,
    // `WeakRef.deref()` never yet answering `undefined` because nothing
    // collects the target, and `Storage.persistTo` writing this engine's own
    // length-prefixed record rather than the old engine's pickle format. Every
    // one of those four qualifications lives in the module that implements it. `Error` is
    // above rather than here because the runtime's error family answers for
    // itself, and `AggregateError` joined that family in the change that added
    // it — the list follows the value, never the other way round. The WHATWG
    // group below was added by the change that installed it in `rts-std`.
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
    "EventEmitter",
    "Storage",
    // The web surface, and every one of them follows a VALUE that landed in the
    // same change — which is this list's own rule, stated above. `Blob` and
    // `File` are `node:buffer`'s classes, whole, bound as globals in
    // `rts-node`'s `lib.rs`; `DOMException` is `rts-std`'s
    // `globals/dom_exception.rs`, built through the program's own `Error` so
    // `instanceof Error` holds; `Headers`, `FormData`, `Request` and `Response`
    // are `rts-std`'s `globals/fetch/`.
    //
    // `Blob` is named a few lines up as an example of a name reached by the
    // suite and deliberately ABSENT "until something supplies them". Something
    // has; the sentence above is left standing because `fetch`, `require`,
    // `FinalizationRegistry`, `eval` and `Intl` are still in exactly that
    // position. `fetch` in particular is absent BY DECISION rather than for want
    // of work — `globals/fetch/mod.rs` says why a `fetch` that cannot reach the
    // network must not have the name.
    "Blob",
    "File",
    "DOMException",
    "Headers",
    "FormData",
    "Request",
    "Response",
    // `globalThis.crypto` — `rts-node`'s `crypto/webcrypto.rs`, over the digest
    // `createHash` already computed. `crypto.subtle.digest` is exact; every
    // other `SubtleCrypto` member is absent and fails at the call, which is the
    // asymmetry this list's comment describes from the other side.
    "crypto",
    // Installed by `rts-std`'s `globals/timing.rs`, and absent from this
    // list until now — so a program writing it with no import was refused at
    // compile time for a name that had a value all along. The two sets are
    // deliberately separate, which is what let them drift; that module's doc
    // names this list as the piece its caller has to add.
    "queueMicrotask",
    // `rts-std`'s `globals/pump.rs` — devolver o controle ao laço do motor de
    // dentro de um laço do programa. Entrou aqui na mesma mudança que o
    // instalou, que é o que `queueMicrotask` acima ensina a fazer: o valor
    // existia e o nome era recusado, e a diferença só aparece na CHAMADA — um
    // `typeof` do mesmo nome respondia `"function"`.
    "pumpEvents",
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

/// Emits a read of a name the compiler could not place anywhere at all.
///
/// # Why this is not [`EmitError::UnboundName`] any more
///
/// It used to be, unconditionally, and the comment at the top of this module
/// called that **stricter than the language** on purpose: the read the
/// specification gives this name is a `ReferenceError`, catchable, raised only
/// when the read actually runs — and answering that at COMPILE time refuses a
/// program whose read is on a path never taken (a dead UMD branch checking
/// `typeof exports`) or whose name reaches the reader only through
/// `globalThis`, never through a declaration this crate's scope analysis can
/// see.
///
/// # Why this does not cost the typo the compile-time catch bought
///
/// Nothing about *when* a name resolves changed — only what happens once it
/// does not. `binding::read` still asks the scope, then [`predefined`], then
/// [`resolves`] (`PROVIDED` plus every sloppy write `sloppy::created` found)
/// before reaching here, and every one of those is exactly what decided the
/// compile-time refusal before. A name a program *could* have meant as a local
/// was never unbound by any of those tests to begin with — a declared `let`,
/// a parameter, a hoisted `function`, all resolve in the scope walk and never
/// reach this. What reaches here is a name none of that machinery placed
/// anywhere, which is the language's own definition of "reading this throws",
/// not a heuristic guess at which typo the author meant.
pub(super) fn unbound_read(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    name: Name,
) -> EmitResult<ValueId> {
    let key = key_constant(builder, ctx, name);
    Ok(call(builder, ctx, RuntimeOp::UnboundGlobalGet, &[key])?[0])
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
