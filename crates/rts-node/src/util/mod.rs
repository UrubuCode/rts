//! `node:util` — a value formatter, an argument parser, and what stands on
//! them.
//!
//! # What reuse-check found
//!
//! Searched `rts-cranelift` by concern first, as the skill's table says — tags,
//! shapes, ABI, GC, scheduling. **Nothing there answers any of this**, and that
//! is right rather than a gap: formatting a value, parsing a flag and stripping
//! an escape sequence are all above the machine.
//!
//! Searched `rts-core`'s host surface next (`entry/modules.rs` and every
//! `pub use` in `entry/mod.rs`, read in full). Three findings changed what this
//! module does:
//!
//! - `entry::is_array` is the REAL `Array.isArray` — the array side table, not
//!   a shape guess. An earlier version of this file said that table was
//!   unreachable and printed `{0: "a", 1: "b"}` as an array because of it. That
//!   divergence is gone, in the formatter and in `types.isArray` alike.
//! - `entry::bytes_of` answers `Some` for exactly a `TypedArray` or a
//!   `DataView`, which IS `ArrayBuffer.isView` — the one brand `util.types` can
//!   ask for honestly. See `types.rs` for why the other thirty-nine cannot.
//! - `entry::module_specifiers`/`module_at_name` enumerate every module
//!   namespace object, which answers `types.isModuleNamespaceObject` exactly.
//!
//! Searched this crate for the members that look like someone else's:
//! `assert.rs` has a `deepStrictEqual` but it is a *thrower* over the same idea
//! and its comparator is `pub(self)` — nothing to reach, so the walk here stays
//! here; `crate::fs::options_and_listener` is the overload shifter and no
//! function in `node:util` has an optional-middle-argument overload, so it is
//! not used. `TextEncoder`/`TextDecoder` live in `rts-std`'s globals and are
//! refused here — see below for why re-exporting them is not possible yet.
//!
//! # The rule this module tree is written around
//!
//! An `extern "C"` frame cannot unwind, so a second runtime borrow does not
//! error — it **aborts the process**. Every reader here (`own_keys`,
//! `get_indexed`, `type_of`, `described`, `is_array`, `to_boolean`) takes its
//! own borrow and is therefore called with none held; every builder
//! (`make_object`, `put_member`, `make_array_in`, `make_string`) needs one held.
//! So every native has one shape: **read ambiently, compute in Rust, build
//! inside one borrow**. `values.rs` states it once and the readers there take no
//! context, so a caller cannot get it wrong by forgetting.
//!
//! # Not implemented, by name
//!
//! Each of these answers `undefined` — a value a program can test — rather than
//! a plausible wrong one.
//!
//! Two claims that used to stand here were both stale rather than wrong-headed,
//! and both are gone. `entry::closure_new` gives a native a captured
//! environment, so a wrapper CAN remember what it wraps — `deprecate`,
//! `debuglog`, `debug` and `callbackify` all work, in `wrap.rs`. And
//! `entry::promise_new`/`entry::promise_settle` mint a PENDING promise and settle
//! it later, so **`promisify` works too** — `promisify.rs`, which states its own
//! divergences and why the reference document's multi-value tupling was
//! rejected.
//!
//! ## Needs a brand the host surface does not expose
//!
//! Every `util.types.*` member except `isArray`, `isArrayBufferView` and
//! `isModuleNamespaceObject`: `isAnyArrayBuffer`, `isArgumentsObject`,
//! `isArrayBuffer`, `isAsyncFunction`, `isBigInt64Array`, `isBigIntObject`,
//! `isBigUint64Array`, `isBooleanObject`, `isBoxedPrimitive`, `isCryptoKey`,
//! `isDataView`, `isDate`, `isExternal`, `isFloat16Array`, `isFloat32Array`,
//! `isFloat64Array`, `isGeneratorFunction`, `isGeneratorObject`, `isInt8Array`,
//! `isInt16Array`, `isInt32Array`, `isKeyObject`, `isMap`, `isMapIterator`,
//! `isNativeError`, `isNumberObject`, `isPromise`, `isProxy`, `isRegExp`,
//! `isSet`, `isSetIterator`, `isSharedArrayBuffer`, `isStringObject`,
//! `isSymbolObject`, `isTypedArray`, `isUint8Array`, `isUint8ClampedArray`,
//! `isUint16Array`, `isUint32Array`, `isWeakMap`, `isWeakSet`. `types.rs` names
//! the rejected alternative and why it lost.
//!
//! ## Needs something this engine does not have
//!
//! - **`TextEncoder` / `TextDecoder`.** They exist — `rts-std`'s
//!   `globals/text.rs` implements both and declares them as globals — and this
//!   module still cannot name them: a host module has no way to READ a global
//!   by name (`entry::global_get` takes an interned key number nothing here can
//!   mint), and `entry::make_prototype` is idempotent BY NAME, so asking it for
//!   `"TextEncoder"` would mint an empty prototype and win the name from the
//!   real class. Re-exporting them needs one host accessor, not code here.
//!   A program reaching the ambient globals directly gets the real classes.
//! - **`getCallSites`.** No stack walk, and no `scriptId` concept to synthesize
//!   one from.
//! - **`getSystemErrorName` / `getSystemErrorMap` / `getSystemErrorMessage`.**
//!   These read libuv's errno table, and that table is *not* platform-
//!   independent the way the reference document claims: libuv defines
//!   `UV_ENOENT` as `-ENOENT` on POSIX and as `-4058` on Windows. A table
//!   embedded here would be right on one platform and silently wrong on the
//!   other. `getSystemErrorMap` is refused twice over — nothing here can
//!   construct a `Map`.
//! - **`setTraceSigInt`.** Installing a `SIGINT` handler is `node:process`'s,
//!   and a second one would fight it.
//! - **`transferableAbortController` / `transferableAbortSignal` /
//!   `aborted`.** No `AbortController` is reachable from here and there is no
//!   transfer-list marking mechanism to mark anything with.
//! - **`MIMEType` / `MIMEParams`.** `params` must be a LIVE object whose
//!   mutation changes the owning type's serialization, and whose
//!   `entries`/`keys`/`values` answer iterators — which needs both captured
//!   state and a mintable `Symbol.iterator`, neither of which exists here. A
//!   `MIMEType` whose `params` silently does not write back is the wrong answer
//!   that runs.
//! - **`diff`.** The reference document's own §7 leaves the tokenization of the
//!   string form unverified, and a diff at the wrong granularity is a wrong
//!   answer that runs rather than an obvious gap.
//! - **The `node:sys` alias (DEP0025).** Registering a specifier happens in
//!   `lib.rs`, which this module does not own.
//!
//! ## Implemented, with a stated divergence
//!
//! - **`inspect`'s options.** `depth`, `sorted` and `maxArrayLength` are
//!   honoured. `showHidden`, `colors`, `customInspect`, `showProxy`,
//!   `maxStringLength`, `breakLength`, `compact`, `getters` and
//!   `numericSeparator` are read and ignored, and output is therefore always
//!   uncoloured, always one line, and never truncated by string length.
//!   `depth: null` is walked to 64 levels, not infinitely — see `inspect.rs`.
//! - **`inspect.custom` / `inspect.defaultOptions` / `promisify.custom`.**
//!   Absent AS VALUES. The first and third need a `Symbol` no native can mint;
//!   the second would be a mutable object nothing consults, which is the silent
//!   no-op this crate refuses. `inspect.colors` and `inspect.styles` ARE present,
//!   because they are data and correct as data. `promisify.custom` differs from
//!   the other two in one way that matters: the HOOK is honoured under its other
//!   documented spelling, `fn[Symbol.for('nodejs.util.promisify.custom')]`, and
//!   `promisify.rs` says what single host accessor would let the name answer the
//!   symbol itself.
//! - **A cycle prints `[Circular]`**, not Node's numbered `<ref *1>` /
//!   `[Circular *1]` back-references.
//! - **`format`'s argument count.** Four ABI slots hold the pattern and three
//!   substitutions; `formatWithOptions` holds two, the options bag taking one.
//!   A fourth argument is not silently dropped — it never arrives.
//! - **`%d` and `%f`** answer `NaN` for a string, where Node coerces with
//!   `Number()`. `%s` on an object inspects it at the full depth rather than
//!   Node's `depth: 0`.
//! - **`isDeepStrictEqual`** compares own enumerable properties structurally and
//!   never prototypes, so it already behaves as `skipPrototype: true` and the
//!   option selects nothing. It stops at 32 levels; deeper structures answer
//!   false rather than recursing without bound.
//! - **`styleText`** probes THIS process's stdout whatever `options.stream`
//!   names, because nothing maps a JavaScript stream back to a descriptor.
//! - **`parseArgs`** answers `undefined` for a strict-mode violation instead of
//!   throwing `ERR_PARSE_ARGS_UNKNOWN_OPTION` / `ERR_PARSE_ARGS_INVALID_
//!   OPTION_VALUE`, so those codes are unobservable. Nothing in this engine can
//!   throw catchably: `entry::throw` ends the program.
//! - **`inherits`** answers `undefined` for a non-function argument where Node
//!   throws `TypeError`, for the same reason.
//! - **`convertProcessSignalToExitCode`** knows the twelve signal numbers that
//!   are identical on every Unix and answers `0` for the rest — see
//!   `signals.rs`.
//! - **`toUSVString`** is the identity on any string it can read; a string
//!   holding a lone surrogate cannot cross the host boundary at all and answers
//!   `undefined`.
//! - **`deprecate`'s warning** is written to stderr rather than emitted as a
//!   `process.on('warning')` event, and the `--no-deprecation` /
//!   `--trace-deprecation` / `--throw-deprecation` launch flags are not
//!   consulted. Dedup by `code` IS honoured.
//! - **`debuglog`'s optimizer callback** runs when `debuglog` is called for an
//!   enabled section, not lazily on first use — the property callers rely on
//!   (nothing expensive runs for a disabled section) holds; only the moment
//!   differs.
//! - **`callbackify`** passes three arguments plus the callback, and a FALSY
//!   rejection reason reaches the callback as it is, where Node wraps it in a
//!   fresh `Error` carrying `.reason`. Nothing here can construct an `Error`
//!   the way the language does, and an object shaped like one would be a
//!   fabricated value.

mod args;
mod call_site;
mod equal;
mod inspect;
mod legacy;
mod promisify;
mod signals;
mod style;
mod types;
mod values;
mod wrap;

use rts_core::entry::{self, Context, Provided};

/// The namespace `node:util` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("format", inspect::format),
        ("formatWithOptions", inspect::format_with_options),
        ("isDeepStrictEqual", equal::is_deep_strict_equal),
        ("toUSVString", equal::to_usv_string),
        ("inherits", legacy::inherits),
        ("isArray", legacy::is_array),
        ("_extend", legacy::extend),
        ("parseArgs", args::parse_args),
        ("parseEnv", style::parse_env),
        ("styleText", style::style_text),
        ("stripVTControlCharacters", style::strip_vt),
        ("convertProcessSignalToExitCode", signals::convert_signal),
        ("deprecate", wrap::deprecate),
        ("callbackify", wrap::callbackify),
        ("promisify", promisify::promisify),
        ("debuglog", wrap::debuglog),
        // The CURRENT `util.debug` is an alias of `debuglog`, not the removed
        // DEP0028 single-string printer that shares its name — the reference
        // document calls this out as the module's one real name collision.
        ("debug", wrap::debuglog),
    ];
    // As duas grafias de `getCallSite` juntam-se aqui e não à lista acima
    // porque são um membro sob dois nomes: `call_site::MEMBERS` diz isso uma
    // vez, onde duas linhas soltas diriam duas.
    let members: Vec<(&str, Provided)> = members
        .iter()
        .copied()
        .chain(call_site::MEMBERS.iter().copied())
        .collect();
    let namespace = entry::make_namespace(context, &members);
    // `inspect` is built apart from the list because it carries members of its
    // own, and `make_namespace` installs plain callables.
    let inspector = entry::make_callable(context, inspect::inspect);
    let colors = style::colors_object(context);
    entry::put_member(context, inspector, "colors", colors);
    let styles = style::styles_object(context);
    entry::put_member(context, inspector, "styles", styles);
    entry::put_member(context, namespace, "inspect", inspector);
    let types = entry::make_namespace(context, types::MEMBERS);
    entry::put_member(context, namespace, "types", types);
    namespace
}
