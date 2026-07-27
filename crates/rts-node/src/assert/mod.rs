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

use rts_engine::AbiType::{PolyValue, StrPtr, Void};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn f(name: &str, symbol: &str, args: Vec<AbiType>, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, Void),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::THROWS,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registers the `node:assert` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    let two = || vec![PolyValue, PolyValue];
    e.ns("node:assert")
        .doc(
            "Assertions (node:assert): ok/equal/strictEqual/deepEqual/deepStrictEqual \
             (+ negations), throws/doesNotThrow, ifError, fail. Throw AssertionError.",
        )
        .member(f("ok", "__RTS_FN_NODE_ASSERT_OK", vec![PolyValue], "ok(value: object): void", s::__RTS_FN_NODE_ASSERT_OK as *const u8))
        .member(f("ok", "__RTS_FN_NODE_ASSERT_OK_MSG", vec![PolyValue, StrPtr], "ok(value: object, message: string): void", s::__RTS_FN_NODE_ASSERT_OK_MSG as *const u8))
        .member(f("equal", "__RTS_FN_NODE_ASSERT_EQUAL", two(), "equal(actual: object, expected: object): void", s::__RTS_FN_NODE_ASSERT_EQUAL as *const u8))
        .member(f("notEqual", "__RTS_FN_NODE_ASSERT_NOT_EQUAL", two(), "notEqual(actual: object, expected: object): void", s::__RTS_FN_NODE_ASSERT_NOT_EQUAL as *const u8))
        .member(f("strictEqual", "__RTS_FN_NODE_ASSERT_STRICT_EQUAL", two(), "strictEqual(actual: object, expected: object): void", s::__RTS_FN_NODE_ASSERT_STRICT_EQUAL as *const u8))
        .member(f("notStrictEqual", "__RTS_FN_NODE_ASSERT_NOT_STRICT_EQUAL", two(), "notStrictEqual(actual: object, expected: object): void", s::__RTS_FN_NODE_ASSERT_NOT_STRICT_EQUAL as *const u8))
        .member(f("deepEqual", "__RTS_FN_NODE_ASSERT_DEEP_EQUAL", two(), "deepEqual(actual: object, expected: object): void", s::__RTS_FN_NODE_ASSERT_DEEP_EQUAL as *const u8))
        .member(f("deepStrictEqual", "__RTS_FN_NODE_ASSERT_DEEP_STRICT_EQUAL", two(), "deepStrictEqual(actual: object, expected: object): void", s::__RTS_FN_NODE_ASSERT_DEEP_STRICT_EQUAL as *const u8))
        .member(f("notDeepEqual", "__RTS_FN_NODE_ASSERT_NOT_DEEP_EQUAL", two(), "notDeepEqual(actual: object, expected: object): void", s::__RTS_FN_NODE_ASSERT_NOT_DEEP_EQUAL as *const u8))
        .member(f("notDeepStrictEqual", "__RTS_FN_NODE_ASSERT_NOT_DEEP_STRICT_EQUAL", two(), "notDeepStrictEqual(actual: object, expected: object): void", s::__RTS_FN_NODE_ASSERT_NOT_DEEP_STRICT_EQUAL as *const u8))
        .member(f("throws", "__RTS_FN_NODE_ASSERT_THROWS", vec![PolyValue], "throws(fn: object): void", s::__RTS_FN_NODE_ASSERT_THROWS as *const u8))
        .member(f("doesNotThrow", "__RTS_FN_NODE_ASSERT_DOES_NOT_THROW", vec![PolyValue], "doesNotThrow(fn: object): void", s::__RTS_FN_NODE_ASSERT_DOES_NOT_THROW as *const u8))
        .member(f("ifError", "__RTS_FN_NODE_ASSERT_IF_ERROR", vec![PolyValue], "ifError(value: object): void", s::__RTS_FN_NODE_ASSERT_IF_ERROR as *const u8))
        .member(f("match", "__RTS_FN_NODE_ASSERT_MATCH", two(), "match(string: object, regexp: object): void", s::__RTS_FN_NODE_ASSERT_MATCH as *const u8))
        .member(f("match", "__RTS_FN_NODE_ASSERT_MATCH_MSG", vec![PolyValue, PolyValue, StrPtr], "match(string: object, regexp: object, message: string): void", s::__RTS_FN_NODE_ASSERT_MATCH_MSG as *const u8))
        .member(f("doesNotMatch", "__RTS_FN_NODE_ASSERT_DOES_NOT_MATCH", two(), "doesNotMatch(string: object, regexp: object): void", s::__RTS_FN_NODE_ASSERT_DOES_NOT_MATCH as *const u8))
        .member(f("doesNotMatch", "__RTS_FN_NODE_ASSERT_DOES_NOT_MATCH_MSG", vec![PolyValue, PolyValue, StrPtr], "doesNotMatch(string: object, regexp: object, message: string): void", s::__RTS_FN_NODE_ASSERT_DOES_NOT_MATCH_MSG as *const u8))
        .member(f("fail", "__RTS_FN_NODE_ASSERT_FAIL", Vec::new(), "fail(): void", s::__RTS_FN_NODE_ASSERT_FAIL as *const u8))
        .member(f("fail", "__RTS_FN_NODE_ASSERT_FAIL_MSG", vec![StrPtr], "fail(message: string): void", s::__RTS_FN_NODE_ASSERT_FAIL_MSG as *const u8))
        .done();
}
