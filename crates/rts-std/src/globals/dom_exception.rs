//! `DOMException` — the error every WHATWG surface raises.
//!
//! # What reuse-check found, and why nothing here is a second error class
//!
//! `rts-core`'s `entry::throw` already builds errors **through the program's
//! own constructors** (`make_named_error`), which is what makes
//! `e instanceof Error` hold and what captures `.stack` where every other error
//! captures it. So this file mints no error object of its own: it asks for a
//! plain `Error`, relinks it to a prototype that inherits from
//! `Error.prototype`, and stamps the two fields a `DOMException` adds. Building
//! a standalone class here would have produced something a `catch (e)` could not
//! recognise as an error at all.
//!
//! Nothing in `rts-cranelift` answers this — an error class is a runtime object,
//! not anything the compiler emits — and `rts-node` has no `DOMException`
//! either: three places in this workspace name its ABSENCE in prose
//! (`globals/events/abort.rs`, `globals/events/mod.rs`, `node:buffer`), which is
//! how this file came to be written rather than a fourth workaround.
//!
//! # Why it is here rather than in `rts-core` or `rts-node`
//!
//! The same availability line the rest of this folder is on. `DOMException` is
//! in no ECMA-262 section, so it is not the runtime's; it is WHATWG rather than
//! Node's, so it is not `rts-node`'s. It is host furniture, like `Event` and
//! `AbortController` beside it — and those two are its first callers.
//!
//! # Why `code` is a table and not a getter over a table
//!
//! Because the value cannot change: `name` is read-only in the specification and
//! the legacy code is a pure function of it. A getter would be re-deriving a
//! constant on every read, and the property is stamped once at construction for
//! the same reason `Blob` stamps `size`.
//!
//! # Not implemented, by name
//!
//! - **`name`/`message`/`code` as read-only inherited accessors.** They are own
//!   data properties here, so `Object.keys(new DOMException(""))` answers three
//!   names where Node answers none, and a program may assign to them. The same
//!   divergence `globals/text.rs` states for `TextDecoder`.
//! - **`QuotaExceededError` as a subclass.** Node v26 makes `DOMException` its
//!   parent; this engine targets Node 25 parity, so the name is a legacy code
//!   (22) here and not a class.
//! - **`DOMException` with a non-string `name` argument.** `String(name)` is
//!   applied, which is the specification's `DOMString` conversion, but an object
//!   whose `toString` is user code answers `undefined` — the boundary every
//!   host-side coercion in this workspace stops at.

use rts_core::entry::{self, Context, Provided};

/// The legacy numeric codes, from the WHATWG "error names table".
///
/// A name absent from it answers `0`, which is what the table itself says for
/// every name added after the legacy codes were frozen. Written as a slice
/// rather than a match so the pair is stated once and read once — the constants
/// installed on the class below are derived from the SAME rows, which is what
/// keeps `DOMException.ABORT_ERR` and `new DOMException("", "AbortError").code`
/// from disagreeing.
///
/// `docs/reference/node/globals.md` §2.16 says `AbortError` and `TimeoutError`
/// have no legacy code; that is wrong, and it was checked against a running
/// engine rather than argued — both Node and Bun answer 20 and 23. The
/// constants list in the same section is right and is what the third column
/// below comes from.
const CODES: &[(&str, &str, f64)] = &[
    ("IndexSizeError", "INDEX_SIZE_ERR", 1.0),
    ("", "DOMSTRING_SIZE_ERR", 2.0),
    ("HierarchyRequestError", "HIERARCHY_REQUEST_ERR", 3.0),
    ("WrongDocumentError", "WRONG_DOCUMENT_ERR", 4.0),
    ("InvalidCharacterError", "INVALID_CHARACTER_ERR", 5.0),
    ("", "NO_DATA_ALLOWED_ERR", 6.0),
    ("NoModificationAllowedError", "NO_MODIFICATION_ALLOWED_ERR", 7.0),
    ("NotFoundError", "NOT_FOUND_ERR", 8.0),
    ("NotSupportedError", "NOT_SUPPORTED_ERR", 9.0),
    ("InUseAttributeError", "INUSE_ATTRIBUTE_ERR", 10.0),
    ("InvalidStateError", "INVALID_STATE_ERR", 11.0),
    ("SyntaxError", "SYNTAX_ERR", 12.0),
    ("InvalidModificationError", "INVALID_MODIFICATION_ERR", 13.0),
    ("NamespaceError", "NAMESPACE_ERR", 14.0),
    ("InvalidAccessError", "INVALID_ACCESS_ERR", 15.0),
    ("", "VALIDATION_ERR", 16.0),
    ("TypeMismatchError", "TYPE_MISMATCH_ERR", 17.0),
    ("SecurityError", "SECURITY_ERR", 18.0),
    ("NetworkError", "NETWORK_ERR", 19.0),
    ("AbortError", "ABORT_ERR", 20.0),
    ("URLMismatchError", "URL_MISMATCH_ERR", 21.0),
    ("QuotaExceededError", "QUOTA_EXCEEDED_ERR", 22.0),
    ("TimeoutError", "TIMEOUT_ERR", 23.0),
    ("InvalidNodeTypeError", "INVALID_NODE_TYPE_ERR", 24.0),
    ("DataCloneError", "DATA_CLONE_ERR", 25.0),
];

/// The legacy code a `name` carries, `0` for one the table does not list.
///
/// Two rows carry an empty name on purpose: `DOMSTRING_SIZE_ERR` and the other
/// three retired codes have a constant and no name that produces them, and an
/// empty `name` argument defaults to `"Error"` before this is ever asked — so
/// no lookup can match one.
fn code_of(name: &str) -> f64 {
    CODES
        .iter()
        .find(|(listed, _, _)| !listed.is_empty() && *listed == name)
        .map_or(0.0, |(_, _, code)| *code)
}

const METHODS: &[(&str, Provided)] = &[];

/// Installs `DOMException` as a global.
pub fn install(context: &mut Context) {
    let class = class(context);
    entry::declare_global(context, "DOMException", class);
}

/// The `DOMException` constructor, made once.
///
/// Idempotent through `constructor` on the prototype: `make_prototype` already
/// answers the same prototype for a name, and the constructor recorded on it is
/// both the `DOMException.prototype.constructor` the language specifies and the
/// only per-CONTEXT place a host module may keep one. A `static` would be
/// process-global where a context is per-thread, which is the same reasoning
/// every `Aside`-shaped table in this workspace records.
pub(super) fn class(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "DOMException", METHODS);
    let held = entry::get_member(context, prototype, "constructor");
    // Its own `prototype` has to BE this one. `construct` links this prototype
    // to `Error.prototype`, `get_member` walks the chain, and without the second
    // half of this test a later call would read `Error`'s `constructor` through
    // that link and hand back `Error` as the `DOMException` class.
    if held != entry::undefined_in(context)
        && entry::get_member(context, held, "prototype") == prototype
    {
        return held;
    }
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    entry::put_member(context, prototype, "constructor", ctor);
    // `name` as a data property, because a native callable has none in this
    // engine. It matters more here than anywhere: `e.constructor.name` is how a
    // `catch` block says which error it caught, and the whole point of this
    // class is being caught.
    let held = entry::make_string(context, "DOMException");
    entry::put_member(context, ctor, "name", held);
    // The constants live on BOTH, which the specification requires by name:
    // `DOMException.INDEX_SIZE_ERR` and `err.INDEX_SIZE_ERR` must each resolve,
    // and the second is satisfied by the prototype rather than by stamping 25
    // numbers onto every instance.
    for (_, constant, code) in CODES {
        let value = entry::make_number(*code);
        entry::put_member(context, ctor, constant, value);
        entry::put_member(context, prototype, constant, value);
    }
    ctor
}

/// `new DOMException(message?, name?)`.
///
/// # Why the object is an `Error` first and a `DOMException` second
///
/// `entry::make_named_error` runs the program's own `Error` constructor, so what
/// comes back already has `message`, a `stack` captured at the construction
/// site, and a prototype chain a `catch` recognises. Relinking that object to
/// this class's prototype — whose own parent is the `Error.prototype` it was
/// just wearing — adds `instanceof DOMException` without taking
/// `instanceof Error` away.
///
/// Every ambient call here is OUTSIDE the borrow below, which is not a style
/// choice: `make_named_error`, `get_prototype` and `set_prototype` each take
/// their own, and a second one inside an `extern "C"` frame is a panic that
/// cannot unwind — it aborts the process.
extern "C" fn construct(_e: u64, _this: u64, message: u64, name: u64, _c: u64, _d: u64) -> u64 {
    let message_text = text_argument(message).unwrap_or_default();
    // `"Error"`, not the empty string: that is what a `DOMException` built with
    // no name reports, in Node and in every browser.
    let name_text = text_argument(name).unwrap_or_else(|| "Error".to_owned());
    let Some(error) = entry::make_named_error("Error", &message_text) else {
        return entry::undefined_value();
    };
    let error_prototype = entry::get_prototype(error);
    let prototype = entry::with_runtime(|context| entry::make_prototype(context, "DOMException", METHODS));
    // Unconditional rather than guarded by a "was it linked" read: relinking to
    // what is already there is a no-op `entry::set_prototype` answers without
    // touching the cell, so the guard would be a second lookup for a question
    // the one call already asks.
    entry::set_prototype(prototype, error_prototype);
    entry::set_prototype(error, prototype);
    entry::with_runtime(|context| {
        let held = entry::make_string(context, &name_text);
        entry::put_member(context, error, "name", held);
        let code = entry::make_number(code_of(&name_text));
        entry::put_member(context, error, "code", code);
        error
    })
}

/// An argument as text, `None` for an absent one.
///
/// `text_of` is `ToString`, which is what a `DOMString` parameter takes — so
/// `new DOMException(42)` reports `"42"`, as it does in Node.
fn text_argument(value: u64) -> Option<String> {
    let absent = entry::undefined_value();
    match value == absent {
        true => None,
        false => entry::text_of(value),
    }
}

/// A `DOMException` a host module can raise, built the same way `new` builds it.
///
/// # Why the callers needed this
///
/// `AbortController.abort()` and `AbortSignal.timeout()` both owe a
/// `DOMException` as `signal.reason`, and both answered a plain
/// `{ name, message }` object instead — which reads correctly and fails the one
/// test a program actually writes, `reason instanceof DOMException`. They can
/// now build the real one, and this is the entry that lets them do it without
/// reaching for the constructor through the global object.
///
/// Ambient throughout, so a caller holding a context must drop it first.
pub(super) fn make(message: &str, name: &str) -> u64 {
    let (message_value, name_value) =
        entry::with_runtime(|context| (entry::make_string(context, message), entry::make_string(context, name)));
    let absent = entry::undefined_value();
    construct(absent, absent, message_value, name_value, absent, absent)
}
