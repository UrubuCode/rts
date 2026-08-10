//! `util.promisify` — a callback-taking function, answered as a
//! promise-returning one.
//!
//! # What reuse-check found
//!
//! Searched `rts-cranelift` by concern first. **`sched` owns promise identity,
//! the wait sets and settlement**, and nothing here re-derives any of it: this
//! module reaches that machine only through `entry::promise_new` and
//! `entry::promise_settle`, exactly as `crate::timers::promises` does. No second
//! promise table, no second numbering, no queue of its own.
//!
//! Searched this crate and the host surface next, and found **three mechanisms
//! already answering the three hard parts**, so nothing new is invented:
//!
//! - **The wrapper remembering its target** is [`entry::closure_new`], which
//!   takes a code address and an environment value and delivers the environment
//!   as the callee's FIRST argument — the `_e` slot most natives here ignore.
//!   [`super::wrap`] found this first and `perf_hooks::timerify` before it; the
//!   environment is an array read back with the ambient `get_indexed`, one way
//!   rather than two.
//! - **The argument list** is [`entry::rest_arguments`], which answers what the
//!   caller really passed — from the pending vector when there was one, and from
//!   the four convention slots with trailing padding dropped when there was not.
//!   Combined with [`entry::call_with_args`] it removes the four-slot ceiling
//!   from BOTH ends, which is why the callback always has a place to go. The
//!   alternative — scanning `[a0, a1, a2, a3]` for the last present value, which
//!   `callbackify` still does — loses an explicit trailing `undefined` and has no
//!   room for the callback at all when four arguments were passed. It lost on
//!   both counts.
//! - **`promisify.custom`** is `Symbol.for('nodejs.util.promisify.custom')`, and
//!   `rts-core-rwk`'s `entry/symbol.rs` states the wire format: a shared symbol's
//!   key text is `"@@for:" + key`. That text is [`CUSTOM`], and reading or
//!   writing a property under it IS reading or writing the symbol-keyed property
//!   a program's `Symbol.for` reaches. The `@@` prefix keeps it out of
//!   enumeration for free.
//!
//! # Why the promise is minted before the target is called, and outside a borrow
//!
//! `promise_new` and `promise_settle` take the runtime borrow themselves. An
//! `extern "C"` frame cannot unwind, so a second borrow does not error — it
//! ABORTS the process. Every host call in this file is therefore made with no
//! borrow held, and the one place a `Context` is needed is a `with_runtime`
//! containing nothing else.
//!
//! Minting first also matters for a synchronous callee: `fn(cb)` that calls `cb`
//! before returning settles a promise this function has not answered yet, which
//! is correct — a settled promise queues its reactions on attach rather than
//! running them, so `.then` attached afterwards still fires, and fires from the
//! drain rather than inline.
//!
//! # Not implemented, by name
//!
//! - **`util.promisify.custom` as a VALUE.** No host accessor mints a symbol:
//!   `entry/symbol.rs`'s `shared` and `mint` are module-private and nothing
//!   re-exports them, so this module cannot answer the symbol itself. Writing
//!   the key text as a plain string under that name was rejected — it would
//!   answer `typeof === "string"` and break `Symbol.keyFor`, and it would
//!   contradict the same refusal `inspect.custom` already makes two files away.
//!   **The capability is honoured under its other documented spelling**: define
//!   `fn[Symbol.for('nodejs.util.promisify.custom')]` and it is found and
//!   returned. One host accessor — `entry::symbol_for(key: &str) -> u64` over the
//!   existing `symbol::shared` — closes this, and nothing else is missing.
//! - **A `TypeError` for a non-function argument.** `undefined` instead, which
//!   is what every other member of this module does for the reason its doc
//!   states: `entry::throw` ends the program rather than reaching a `catch`.
//! - **The wrapper's `name` and `length`.** Node's copies the original's. There
//!   is no host way to set either on a callable made here.
//! - **Non-enumerable own properties of the original.** Node copies the full
//!   descriptor set onto the wrapper; `entry::own_keys` answers own ENUMERABLE
//!   string keys, so that is what is copied. A getter is copied as the value it
//!   answered, not as a getter.
//! - **Multi-value tupling.** `docs/reference/node/util.md` §2 says
//!   `callback(null, a, b)` may resolve to `[a, b]` and marks it "verify". Node
//!   resolves to `a`; the array form is reached only through the internal
//!   `kCustomPromisifyArgs`, which is not public API. The first value is what is
//!   implemented, and the document's unverified claim is the rejected
//!   alternative — inventing an array where Node hands over a value would be a
//!   wrong answer that runs.

use rts_core_rwk::entry;

use super::values::{array_items, is_callable, own_key_strings, string};

/// The key text `Symbol.for('nodejs.util.promisify.custom')` names a property
/// with.
///
/// Not a guess: `rts-core-rwk`'s `entry/symbol.rs` documents `"@@for:<key>"` as
/// the encoding of a shared symbol and keeps it deliberately disjoint from the
/// well-known space, so this text cannot collide with `Symbol.asyncIterator` or
/// with a private class member.
const CUSTOM: &str = "@@for:nodejs.util.promisify.custom";

/// The environment slot the wrapper reads its target out of.
const TARGET: f64 = 0.0;

/// `util.promisify(original)`.
pub(super) extern "C" fn promisify(
    _e: u64,
    _this: u64,
    original: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    if !is_callable(original) {
        return entry::undefined_value();
    }
    // The custom hook wins outright, before anything is built. That is the whole
    // point of it: Node's own APIs with a nonstandard callback signature use it
    // to hand over a function this generic wrapper would call wrongly.
    let custom = entry::get_indexed(original, string(CUSTOM));
    if is_callable(custom) {
        return custom;
    }
    let environment = entry::make_array(vec![original]);
    let wrapper = entry::closure_new(promisified as *const () as usize as i64, environment);
    copy_own(original, wrapper);
    // `promisify(promisify(f))` answers the same function, which is what Node's
    // own idempotence rests on — it defines the custom hook on the result too.
    // Written under the symbol's key text, so a program asking with
    // `Symbol.for(...)` sees exactly this.
    entry::set_indexed(wrapper, string(CUSTOM), wrapper);
    wrapper
}

/// Copies the original's own enumerable properties onto the wrapper.
///
/// Node copies the full descriptor set. Own enumerable string keys are what
/// `entry::own_keys` can answer, and copying those is closer to Node than
/// copying none — the properties real code reads off a promisified function
/// (`fs.realpath.native`, a module's own tag) are ordinary enumerable data.
fn copy_own(original: u64, wrapper: u64) {
    for name in own_key_strings(original) {
        let key = string(&name);
        let value = entry::get_indexed(original, key);
        entry::set_indexed(wrapper, key, value);
    }
}

/// The promise-returning function `promisify` answers.
extern "C" fn promisified(environment: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let target = entry::get_indexed(environment, entry::make_number(TARGET));
    // Minted first, and outside every borrow. See the module doc for both.
    let promise = entry::promise_new();
    let callback = entry::closure_new(
        settle_from as *const () as usize as i64,
        entry::make_array(vec![promise]),
    );
    // What the caller REALLY passed, rather than four slots with padding in
    // them: `rest_arguments` drops trailing `undefined` when there was no
    // vector and reads the vector when there was, so an explicit fourth
    // argument does not cost the callback its place.
    let mut arguments = array_items(entry::rest_arguments(0, a0, a1, a2, a3));
    arguments.push(callback);
    entry::call_with_args(target, this, entry::make_array(arguments));
    promise
}

/// The node-style callback the target is handed: `(err, value)`.
///
/// Truthiness of `err` decides, which is Node's own rule and not a null check —
/// a callback answering `callback(0)` has not failed, and one answering
/// `callback(someError)` has.
extern "C" fn settle_from(
    environment: u64,
    _this: u64,
    error: u64,
    value: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let promise = entry::get_indexed(environment, entry::make_number(TARGET));
    match entry::to_boolean(error) {
        true => entry::promise_settle(promise, error, 1),
        false => entry::promise_settle(promise, value, 0),
    };
    entry::undefined_value()
}
