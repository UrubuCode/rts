//! `WebAssembly.CompileError`, `LinkError` and `RuntimeError`.
//!
//! The three are one shape — a class whose prototype's parent is
//! `Error.prototype` — so they are built by one function rather than written
//! three times. Which of them a failure raises is the JS-API's own division:
//! decoding or validating raises `CompileError`, satisfying imports raises
//! `LinkError`, and a trap during a call raises `RuntimeError`.
//!
//! # Why `make_named_error` and not a plain object
//!
//! The same reason `dom_exception.rs` gives: an object built by the program's own
//! `Error` constructor arrives with `message`, a `stack` captured where it was
//! constructed, and a chain a `catch` recognises — and relinking it to this
//! class's prototype adds `instanceof CompileError` without taking
//! `instanceof Error` away. A hand-built `{ name, message }` reads correctly and
//! fails the one thing a program writes.

use rts_core::entry;

/// The three, in the order [`install`] registers them.
pub(super) const NAMES: [&str; 3] = ["CompileError", "LinkError", "RuntimeError"];

/// Which of the three a failure is.
#[derive(Clone, Copy)]
pub(super) enum Which {
    Compile,
    Link,
    Runtime,
}

impl Which {
    /// All three, which is what `install` walks.
    pub(super) const ALL: [Which; 3] = [Which::Compile, Which::Link, Which::Runtime];

    pub(super) fn name(self) -> &'static str {
        NAMES[self.index()]
    }

    /// Its position in [`NAMES`], which is also what a constructor closes over.
    pub(super) fn index(self) -> usize {
        match self {
            Which::Compile => 0,
            Which::Link => 1,
            Which::Runtime => 2,
        }
    }

    /// The body `class` installs for it.
    fn construct(self) -> entry::Provided {
        match self {
            Which::Compile => compile_error,
            Which::Link => link_error,
            Which::Runtime => runtime_error,
        }
    }
}

/// Builds one of the three, already linked, without raising it.
///
/// Every call is ambient: `make_named_error`, `get_prototype` and `set_prototype`
/// each take their own borrow, and a second one inside an `extern "C"` frame
/// aborts the process rather than unwinding.
pub(super) fn make(which: Which, message: &str) -> u64 {
    let name = which.name();
    let Some(error) = entry::make_named_error("Error", message) else {
        return entry::undefined_value();
    };
    // The prototype `class` already built and linked to `Error.prototype`;
    // `make_prototype` answers that one rather than a second object, which is
    // what makes `caught instanceof WebAssembly.CompileError` hold.
    let prototype = entry::with_runtime(|context| entry::make_prototype(context, name, &[]));
    entry::set_prototype(error, prototype);
    entry::with_runtime(|context| {
        let held = entry::make_string(context, name);
        entry::put_member(context, error, "name", held);
    });
    error
}

/// Raises one of the three where a `catch` in the program can see it.
pub(super) fn raise(which: Which, message: &str) {
    entry::throw_value(make(which, message));
}

/// The constructor and prototype of one of the three, for the namespace.
///
/// A program reads `WebAssembly.CompileError` for `instanceof`, and that needs
/// the constructor to be the one whose `prototype` is what [`make`] links to —
/// hence the same `make_prototype(name)` call, which answers the one prototype
/// held under that name rather than a second object of the same shape.
///
/// # Why this takes the context and [`make`] does not
///
/// `install` runs BEFORE a context is installed on the thread — the global
/// surface is built while the one being built is the argument — so an ambient
/// call here aborts with "an entry point ran with no context installed". Every
/// install-time path therefore threads the `&mut Context`, and only the run-time
/// paths ([`make`], [`raise`]) are ambient, where a context is guaranteed.
///
/// # Why three bodies and not one over an environment
///
/// `instance.rs` tells one exported wasm function from another by the
/// ENVIRONMENT a callable closes over, through `entry::closure_new`. That is not
/// available here: `closure_new` is ambient, and `install` runs before a context
/// exists on the thread. Three classes are a fixed, small set, so three
/// one-line bodies cost nothing — where an export's count is the module's and
/// a table of pre-generated trampolines would cap it.
pub(super) fn class(context: &mut entry::Context, which: Which) -> u64 {
    let name = which.name();
    let prototype = entry::make_prototype(context, name, &[]);
    if let Some(parent) = entry::error_prototype(context) {
        entry::set_prototype_in(context, prototype, parent);
    }
    let ctor = entry::make_callable(context, which.construct());
    entry::put_member(context, ctor, "prototype", prototype);
    entry::declare_host_class(context, ctor, prototype, name, 1);
    ctor
}

/// `new WebAssembly.CompileError(message)` and its two siblings — the
/// constructors a program may call itself, which the JS-API says behave like
/// `Error`.
extern "C" fn compile_error(_e: u64, _this: u64, message: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    make(Which::Compile, &message_text(message))
}

extern "C" fn link_error(_e: u64, _this: u64, message: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    make(Which::Link, &message_text(message))
}

extern "C" fn runtime_error(_e: u64, _this: u64, message: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    make(Which::Runtime, &message_text(message))
}

/// The first argument as a message. `ToString`, which is what a `DOMString`
/// parameter takes, so `new CompileError(42)` reports `"42"` as it does in Node.
fn message_text(value: u64) -> String {
    match value == entry::undefined_value() {
        true => String::new(),
        false => entry::text_of(value).unwrap_or_default(),
    }
}

/// A `TypeError` as a VALUE, for a promise that must reject with one.
///
/// `entry::throw_type_error` raises, which is wrong for `instantiate`: the
/// JS-API's async form rejects rather than throwing, so the error has to be built
/// and handed to `promise_settle`. `make_named_error` is the same route the three
/// wasm classes take, without the relinking.
pub(super) fn make_type_error(message: &str) -> u64 {
    entry::make_named_error("TypeError", message).unwrap_or_else(entry::undefined_value)
}
