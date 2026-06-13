//! Increment 4 proof: REAL `.ts` programs run end to end and print correctly.
//!
//! Each test feeds an ACTUAL TS source string to [`super::run_source`] — parse →
//! rts-hir → run-lowering (Tagged path) → whole-module JIT → execute — and
//! asserts the EXACT captured stdout against what Node/Bun would print. These are
//! the first programs the new engine runs to completion with output.
//!
//! Out-of-subset constructs bail with an explicit `Unsupported` (the negative
//! tests at the bottom), never a silent wrong value.

use super::{render_source, run_source};

/// Run `src` (console.log captured via the real-pool-backed sink) and assert its
/// rendered stdout equals `expected`.
fn assert_stdout(src: &str, expected: &str) {
    match render_source(src) {
        Ok(out) => assert_eq!(out, expected, "stdout mismatch for source:\n{src}"),
        Err(e) => panic!("render_source failed for:\n{src}\n  -> {e}"),
    }
}

// ===========================================================================
// Numeric + string `+` through the generic path.
// ===========================================================================

#[test]
fn add_numbers() {
    assert_stdout("console.log(1 + 2);", "3\n");
}

#[test]
fn concat_strings() {
    assert_stdout(r#"console.log("a" + "b");"#, "ab\n");
}

#[test]
fn number_plus_string() {
    assert_stdout(r#"console.log(1 + "x");"#, "1x\n");
}

// ===========================================================================
// typeof — multiple args, mixed kinds.
// ===========================================================================

#[test]
fn typeof_mixed() {
    assert_stdout(
        r#"console.log(typeof 1, typeof "s", typeof true);"#,
        "number string boolean\n",
    );
}

// ===========================================================================
// JS number formatting: fractional vs integer-valued.
// ===========================================================================

#[test]
fn float_formatting() {
    assert_stdout("console.log(1.5);", "1.5\n");
}

#[test]
fn integer_valued_float_formatting() {
    // 3.0 prints as "3" (no decimal) — the headline JS Number→String case.
    assert_stdout("console.log(3.0);", "3\n");
}

// ===========================================================================
// Function defs + cross-function calls.
// ===========================================================================

#[test]
fn single_function_call() {
    assert_stdout(
        "function sq(x: number){ return x*x; } console.log(sq(5));",
        "25\n",
    );
}

#[test]
fn cross_function_call_chain() {
    assert_stdout(
        r#"
        function inc(x: number){ return x + 1; }
        function dbl(x: number){ return x * 2; }
        console.log(dbl(inc(4)));
        "#,
        "10\n",
    );
}

// ===========================================================================
// A loop printing each iteration (top-level control flow + ToBoolean cond).
// ===========================================================================

#[test]
fn loop_printing() {
    assert_stdout(
        "let i = 0; while (i < 3) { console.log(i); i = i + 1; }",
        "0\n1\n2\n",
    );
}

// ===========================================================================
// Strict equality returning booleans.
// ===========================================================================

#[test]
fn strict_eq_booleans() {
    assert_stdout("console.log(true === true, 1 === 2);", "true false\n");
}

// ===========================================================================
// Extra coverage — combinations.
// ===========================================================================

#[test]
fn string_eq() {
    assert_stdout(r#"console.log("ab" === "ab", "a" === "b");"#, "true false\n");
}

#[test]
fn typeof_of_variable() {
    // typeof over a runtime value (not a literal) → runtime tag inspection.
    assert_stdout(
        r#"let s = "hi"; let n = 42; console.log(typeof s, typeof n);"#,
        "string number\n",
    );
}

#[test]
fn if_over_number_truthiness() {
    // `if (n)` with a number condition exercises inline ToBoolean.
    assert_stdout(
        "let n = 5; if (n) { console.log(\"yes\"); } else { console.log(\"no\"); }",
        "yes\n",
    );
}

#[test]
fn string_concat_in_loop() {
    assert_stdout(
        r#"
        let i = 0;
        while (i < 2) {
            console.log("row" + i);
            i = i + 1;
        }
        "#,
        "row0\nrow1\n",
    );
}

#[test]
fn multiple_log_lines() {
    assert_stdout(
        r#"console.log(1); console.log(2); console.log("three");"#,
        "1\n2\nthree\n",
    );
}

#[test]
fn negative_number_formatting() {
    assert_stdout("console.log(-0.0, -5, -2.5);", "0 -5 -2.5\n");
}

/// Smoke the REAL-stdout path (`run_source`, NOT the capture path): it must run
/// to completion without SIGILL/crash, proving `__rtsadp_print_line` forwards to
/// the REAL `__RTS_FN_NS_IO_PRINT(ptr, len)` correctly (the line lands on the
/// test's stdout; this asserts only that the real IO_PRINT branch executes).
#[test]
fn run_source_real_stdout_smoke() {
    let res = run_source(r#"console.log("real-stdout-path", 1 + 2);"#);
    assert!(res.is_ok(), "run_source (real IO_PRINT path) failed: {res:?}");
}

// ===========================================================================
// Negative: out-of-subset constructs bail EXPLICITLY (soundness floor).
// ===========================================================================

#[test]
fn whole_object_log_bails() {
    // P3: object literals now WORK for scalar access, but printing a WHOLE object
    // value has no faithful rendering yet — it must bail, not print `[object
    // Object]` (which diverges from Bun's `{ a: 1 }`).
    let res = run_source("let o = { a: 1 }; console.log(o);");
    assert!(res.is_err(), "whole-object log must bail, got {res:?}");
}

#[test]
fn unknown_method_name_bails() {
    // A method NOT in the Registry mirror on a string receiver bails explicitly
    // (P4 covers a fixed surface; an unknown name is never guessed).
    let res = run_source(r#"console.log("a".notARealMethod());"#);
    assert!(res.is_err(), "unknown method name must bail, got {res:?}");
}

#[test]
fn whole_array_log_bails() {
    // Same: printing a whole array value bails (Bun prints `[ 1, 2, 3 ]`).
    let res = run_source("let a = [1, 2, 3]; console.log(a);");
    assert!(res.is_err(), "whole-array log must bail, got {res:?}");
}

// ===========================================================================
// P3: object/array literals + property/index access (scalar pulls).
// ===========================================================================

#[test]
fn object_property_read() {
    assert_stdout("let o = {a: 1, b: 2}; console.log(o.a + o.b);", "3\n");
}

#[test]
fn object_property_write() {
    assert_stdout("let o = {x: 10}; o.x = o.x + 5; console.log(o.x);", "15\n");
}

#[test]
fn object_string_property() {
    assert_stdout(r#"let p = {name: "rts"}; console.log(p.name);"#, "rts\n");
}

#[test]
fn object_nested_scalar_through_function() {
    assert_stdout(
        "let c = {n: 7}; function dbl(v: number){return v*2;} console.log(dbl(c.n));",
        "14\n",
    );
}

#[test]
fn object_missing_key_is_undefined() {
    assert_stdout("let o = {a: 1}; console.log(o.b);", "undefined\n");
}

#[test]
fn array_index_read() {
    assert_stdout("let a = [10, 20, 30]; console.log(a[0] + a[2]);", "40\n");
}

#[test]
fn array_length() {
    assert_stdout("let a = [10, 20, 30]; console.log(a.length);", "3\n");
}

#[test]
fn array_index_write() {
    assert_stdout("let a = [1, 2, 3]; a[1] = 9; console.log(a[1]);", "9\n");
}

#[test]
fn heterogeneous_array_scalar() {
    assert_stdout(r#"let m = [1, "two", 3]; console.log(m[1]);"#, "two\n");
}

#[test]
fn typeof_object_and_array() {
    assert_stdout("console.log(typeof {}, typeof []);", "object object\n");
}

// ===========================================================================
// P4: data-driven instance-method dispatch (String / Number) via the Registry
// mirror — recv.method(args) → the REAL __RTS_FN_GL_* symbol, no switchboard.
// ===========================================================================

#[test]
fn string_to_upper_case() {
    assert_stdout(r#"console.log("hello".toUpperCase());"#, "HELLO\n");
}

#[test]
fn string_to_lower_case() {
    assert_stdout(r#"console.log("HeLLo".toLowerCase());"#, "hello\n");
}

#[test]
fn string_trim() {
    assert_stdout(r#"console.log("  hi  ".trim());"#, "hi\n");
}

#[test]
fn string_index_of() {
    assert_stdout(r#"console.log("abcabc".indexOf("c"));"#, "2\n");
}

#[test]
fn string_repeat() {
    assert_stdout(r#"console.log("rts".repeat(3));"#, "rtsrtsrts\n");
}

#[test]
fn string_includes() {
    assert_stdout(r#"console.log("hello".includes("ell"));"#, "true\n");
    assert_stdout(r#"console.log("hello".includes("zzz"));"#, "false\n");
}

#[test]
fn string_char_at() {
    assert_stdout(r#"console.log("hello".charAt(1));"#, "e\n");
}

#[test]
fn string_slice() {
    assert_stdout(r#"console.log("hello".slice(1, 3));"#, "el\n");
}

#[test]
fn string_starts_ends_with() {
    assert_stdout(r#"console.log("hello".startsWith("he"));"#, "true\n");
    assert_stdout(r#"console.log("hello".endsWith("lo"));"#, "true\n");
}

#[test]
fn string_char_code_at() {
    assert_stdout(r#"console.log("A".charCodeAt(0));"#, "65\n");
}

#[test]
fn string_method_in_concat() {
    // The returned string PolyValue flows through the generic `+` path.
    assert_stdout(r#"console.log("a" + "b".toUpperCase());"#, "aB\n");
}

#[test]
fn number_to_fixed() {
    assert_stdout("console.log((3.14159).toFixed(2));", "3.14\n");
}

// ===========================================================================
// P4.5: Array instance methods WITHOUT callbacks — recv.method(args) over the
// engine's own array representation (a real Entry::Vec of boxed PolyValue words),
// via codegen-owned __rtsadp_arr_* trampolines.
// ===========================================================================

#[test]
fn array_index_of() {
    assert_stdout("let a = [10, 20, 30]; console.log(a.indexOf(20));", "1\n");
    assert_stdout("let a = [10, 20, 30]; console.log(a.indexOf(99));", "-1\n");
}

#[test]
fn array_includes() {
    assert_stdout(
        "let a = [1, 2, 3]; console.log(a.includes(2), a.includes(9));",
        "true false\n",
    );
}

#[test]
fn array_at() {
    assert_stdout("let a = [5, 6, 7]; console.log(a.at(0), a.at(-1));", "5 7\n");
}

#[test]
fn array_join() {
    assert_stdout(r#"let a = ["x", "y", "z"]; console.log(a.join("-"));"#, "x-y-z\n");
    assert_stdout(r#"let a = [1, 2, 3]; console.log(a.join(""));"#, "123\n");
}

#[test]
fn array_push() {
    assert_stdout(
        "let a = [1, 2]; console.log(a.push(3)); console.log(a.length);",
        "3\n3\n",
    );
}

#[test]
fn array_pop() {
    assert_stdout(
        "let a = [1, 2, 3]; console.log(a.pop()); console.log(a.length);",
        "3\n2\n",
    );
}

#[test]
fn array_slice() {
    assert_stdout(
        "let a = [1, 2, 3, 4]; let b = a.slice(1, 3); console.log(b.length, b.at(0));",
        "2 2\n",
    );
}

#[test]
fn array_index_of_heterogeneous() {
    assert_stdout(r#"let m = [1, "two", 3]; console.log(m.indexOf("two"));"#, "1\n");
}

#[test]
fn array_map_callback_method_bails() {
    // `.map` takes a callback (function VALUES) — a later increment. BAIL.
    let res = run_source("let a = [1, 2, 3]; console.log(a.map(x => x));");
    assert!(res.is_err(), "array callback method must bail, got {res:?}");
}

#[test]
fn array_method_on_non_array_bails() {
    // `.indexOf` resolved as an Array method requires a proven-array receiver; on
    // a non-array (here a number variable) it is not an array receiver and the
    // number class has no `indexOf` → BAIL.
    let res = run_source("let n = 5; console.log(n.indexOf(1));");
    assert!(res.is_err(), "array method on a non-array must bail, got {res:?}");
}

// ---- Bail tests: callbacks + non-static-receiver + unsupported arity ----

#[test]
fn array_map_callback_bails() {
    // `.map` takes a callback (function VALUES) — a later increment. BAIL.
    let res = run_source("console.log([1, 2, 3].map(x => x));");
    assert!(res.is_err(), "callback method must bail, got {res:?}");
}

#[test]
fn method_on_dynamic_receiver_bails() {
    // A string method on a VARIABLE (kind Unknown — class not statically proven)
    // bails: dynamic receiver-kind dispatch is a later increment.
    let res = run_source(r#"let s = "hi"; console.log(s.toUpperCase());"#);
    assert!(res.is_err(), "dynamic-receiver method must bail, got {res:?}");
}

#[test]
fn one_arg_slice_bails() {
    // `slice(n)` (1-arg form) relies on a runtime default this table does not
    // inject — BAIL rather than guess the end index.
    let res = run_source(r#"console.log("hello".slice(1));"#);
    assert!(res.is_err(), "1-arg slice must bail, got {res:?}");
}

#[test]
fn const_array_and_object_shapes() {
    // `const` initializers record shapes the same as `let` (the fixture corpus
    // overwhelmingly uses `const a = [...]`).
    assert_stdout("const a = [4, 5]; console.log(a[1], a.length);", "5 2\n");
    assert_stdout("const o = {a: 1}; console.log(o.a);", "1\n");
}

#[test]
fn array_index_in_loop() {
    // Read array slots across loop iterations (exercises VEC_GET with a varying
    // index and the Tagged element path).
    assert_stdout(
        r#"
        let a = [5, 6, 7];
        let i = 0;
        while (i < a.length) {
            console.log(a[i]);
            i = i + 1;
        }
        "#,
        "5\n6\n7\n",
    );
}

#[test]
fn object_two_fields_string_and_number() {
    assert_stdout(
        r#"let o = {name: "x", count: 3}; console.log(o.name, o.count);"#,
        "x 3\n",
    );
}

// ===========================================================================
// P3 negative: shape must be statically proven; dynamic access bails.
// ===========================================================================

#[test]
fn member_on_unknown_shape_param_bails() {
    // `o` is a param of unknown shape — a property access on it needs the dynamic
    // inline cache (later increment). Must bail, not guess a slot.
    let res = run_source(
        "function f(o: any){ return o.a; } let r = {a: 1}; console.log(f(r));",
    );
    assert!(res.is_err(), "member on unknown-shape param must bail, got {res:?}");
}

#[test]
fn computed_object_key_bails() {
    // `o[k]` with a dynamic key on a non-array object needs the dynamic property
    // path — must bail.
    let res = run_source(
        r#"let o = {a: 1}; let k = "a"; console.log(o[k]);"#,
    );
    assert!(res.is_err(), "computed dynamic object key must bail, got {res:?}");
}

#[test]
fn adding_new_object_key_bails() {
    // Adding a key not in the literal's shape needs the transition tree — bail.
    let res = run_source("let o = {a: 1}; o.b = 2; console.log(o.b);");
    assert!(res.is_err(), "adding a new object key must bail, got {res:?}");
}

// ---------------------------------------------------------------------------
// Soundness bails forced by HIR ambiguity (the engine REFUSES rather than guess).
// ---------------------------------------------------------------------------

#[test]
fn cross_kind_equality_bails() {
    // `0 == ""` is `true` loose / `false` strict; swc collapses `==`/`===` onto
    // one HIR op, so the engine cannot tell them apart for cross-kind operands.
    // It must bail, not emit a (possibly wrong) boolean.
    let res = run_source(r#"console.log(0 == "");"#);
    assert!(res.is_err(), "cross-kind equality must bail, got {res:?}");
}

#[test]
fn unary_plus_or_not_bails() {
    // swc lowers BOTH unary `+` and `!` to `HirUnOp::Not`; `+"42"` is `42` while
    // `!"42"` is `false`. Indistinguishable in HIR → bail.
    let res = run_source(r#"console.log(+"42");"#);
    assert!(res.is_err(), "unary +/! must bail, got {res:?}");
    let res2 = run_source("console.log(!0);");
    assert!(res2.is_err(), "unary +/! must bail, got {res2:?}");
}
