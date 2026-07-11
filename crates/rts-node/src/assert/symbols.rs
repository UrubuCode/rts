//! node:assert — the `extern "C"` entry points. Values cross as `PolyValue`
//! words; each assertion throws a real `AssertionError` (via the pending-error
//! slot; members are `THROWS`-flagged) on failure, and returns `undefined`
//! otherwise. Optional `message` arguments use `StrPtr` overloads.

use super::words::{
    deep_equal, invoke_and_caught, is_nullish, loose_eq, strict_eq, throw_assertion, truthy,
};

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string()
}

fn fail_msg(default: &str, msg: &str) -> String {
    if msg.is_empty() {
        default.to_string()
    } else {
        msg.to_string()
    }
}

/// `assert(value)` / `assert.ok(value)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_OK(value: u64) {
    if !truthy(value) {
        throw_assertion("The expression evaluated to a falsy value");
    }
}

/// `assert.ok(value, message)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_OK_MSG(value: u64, mp: *const u8, ml: i64) {
    if !truthy(value) {
        throw_assertion(&fail_msg("The expression evaluated to a falsy value", &read(mp, ml)));
    }
}

/// `assert.equal(actual, expected)` — `==`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_EQUAL(a: u64, b: u64) {
    if !loose_eq(a, b) {
        throw_assertion("Expected values to be loosely equal");
    }
}

/// `assert.notEqual(actual, expected)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_NOT_EQUAL(a: u64, b: u64) {
    if loose_eq(a, b) {
        throw_assertion("Expected values to be loosely unequal");
    }
}

/// `assert.strictEqual(actual, expected)` — `===`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_STRICT_EQUAL(a: u64, b: u64) {
    if !strict_eq(a, b) {
        throw_assertion("Expected values to be strictly equal");
    }
}

/// `assert.notStrictEqual(actual, expected)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_NOT_STRICT_EQUAL(a: u64, b: u64) {
    if strict_eq(a, b) {
        throw_assertion("Expected values to be strictly unequal");
    }
}

/// `assert.deepEqual(actual, expected)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_DEEP_EQUAL(a: u64, b: u64) {
    if !deep_equal(a, b, false) {
        throw_assertion("Expected values to be loosely deep-equal");
    }
}

/// `assert.deepStrictEqual(actual, expected)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_DEEP_STRICT_EQUAL(a: u64, b: u64) {
    if !deep_equal(a, b, true) {
        throw_assertion("Expected values to be strictly deep-equal");
    }
}

/// `assert.notDeepEqual(actual, expected)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_NOT_DEEP_EQUAL(a: u64, b: u64) {
    if deep_equal(a, b, false) {
        throw_assertion("Expected values not to be loosely deep-equal");
    }
}

/// `assert.notDeepStrictEqual(actual, expected)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_NOT_DEEP_STRICT_EQUAL(a: u64, b: u64) {
    if deep_equal(a, b, true) {
        throw_assertion("Expected values not to be strictly deep-equal");
    }
}

/// `assert.throws(fn)` — `fn` must throw.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_THROWS(fn_word: u64) {
    if !invoke_and_caught(fn_word) {
        throw_assertion("Missing expected exception");
    }
}

/// `assert.doesNotThrow(fn)` — `fn` must NOT throw.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_DOES_NOT_THROW(fn_word: u64) {
    if invoke_and_caught(fn_word) {
        throw_assertion("Got unwanted exception");
    }
}

/// `assert.ifError(value)` — throws when `value` is not `null`/`undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_IF_ERROR(value: u64) {
    if !is_nullish(value) {
        throw_assertion("ifError got unwanted exception");
    }
}

/// `assert.fail()` — always throws.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_FAIL() {
    throw_assertion("Failed");
}

/// `assert.fail(message)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_ASSERT_FAIL_MSG(mp: *const u8, ml: i64) {
    throw_assertion(&fail_msg("Failed", &read(mp, ml)));
}
