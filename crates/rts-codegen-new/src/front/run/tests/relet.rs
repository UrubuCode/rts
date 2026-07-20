//! Re-`let` classification hygiene (Phase 2): re-binding a local to a DIFFERENT
//! kind must drop the prior kind's classification, so the access path matches the
//! NEW value — not a stale shape/string/class record from the old binding.
//!
//! Before the centralized `clear_local_classifications`, only the numeric tail of
//! `lower_let` removed shape/class maps, and `string_locals` was never cleared, so
//! e.g. `let s = "x"; let s = {…}` left `s` proven-string next to its new object
//! shape and `s.length` wrongly took the string fast path.

use super::assert_stdout;

#[test]
fn relet_string_to_object_uses_object_path() {
    // `s` is first a proven string, then re-bound to an object. `s.n` must read the
    // object field (5), not misroute through the stale string classification.
    assert_stdout("let s = \"hi\"; let s = { n: 5 }; console.log(s.n);", "5\n");
}

#[test]
fn relet_object_to_string_uses_string_path() {
    // `o` is first an object (proven shape), then re-bound to a string. `o.length`
    // must read the STRING length (2), not the stale object shape.
    assert_stdout(
        "let o = { n: 5 }; let o = \"hi\"; console.log(o.length);",
        "2\n",
    );
}

#[test]
fn relet_string_to_number_uses_numeric_path() {
    // string → number: the arithmetic must use the numeric value, with no stale
    // string classification on `x`.
    assert_stdout("let x = \"hi\"; let x = 42; console.log(x + 1);", "43\n");
}
