//! `node:assert` — the assertion function surface. Every assertion throws a real
//! `AssertionError` on failure (via the engine pending-error slot) and returns
//! `undefined` on success. Values cross as `PolyValue` words; `throws`/
//! `doesNotThrow` invoke a function VALUE and inspect the error slot; deep
//! (non-)equality walks the engine value model natively.
//!
//! Deferred (need async / a regex-from-native bridge / a deprecated helper):
//! `rejects`/`doesNotReject` (Promise), `match`/`doesNotMatch` (RegExp arg),
//! `CallTracker` (deprecated), and the constructible `AssertionError` class /
//! the callable default export `assert(value)` (named `ok` covers it). The full
//! synchronous comparison + throws surface is implemented.
//!
//! Module layout: `words` (equality/truthiness/error-slot bridges + deep-equal),
//! `symbols` (extern entry points), `mod` (registration).

mod symbols;
pub(crate) mod words;

use rts_engine::Engine;

/// Registers the `node:assert` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.module("node:assert", |m| {
        m.doc(
            "Assertions (node:assert): ok/equal/strictEqual/deepEqual/deepStrictEqual \
             (+ negations), throws/doesNotThrow, ifError, fail. Throw AssertionError.",
        );
        m.registry(s::ok_entry());
        m.registry(s::ok_msg_entry());
        m.registry(s::equal_entry());
        m.registry(s::not_equal_entry());
        m.registry(s::strict_equal_entry());
        m.registry(s::not_strict_equal_entry());
        m.registry(s::deep_equal__entry());
        m.registry(s::deep_strict_equal_entry());
        m.registry(s::not_deep_equal_entry());
        m.registry(s::not_deep_strict_equal_entry());
        m.registry(s::throws_entry());
        m.registry(s::does_not_throw_entry());
        m.registry(s::if_error_entry());
        m.registry(s::match__entry());
        m.registry(s::match_msg_entry());
        m.registry(s::does_not_match_entry());
        m.registry(s::does_not_match_msg_entry());
        m.registry(s::fail_entry());
        m.registry(s::fail_msg_fn_entry());
    });
}
