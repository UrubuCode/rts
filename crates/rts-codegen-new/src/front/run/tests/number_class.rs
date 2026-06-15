//! The PRIMORDIAL `Number.prototype` methods as a `.ts` prelude class
//! (`rts-primitives/src/number.ts`). A method called on a PRIMITIVE number
//! receiver (`(5).toFixed(2)`, `(255).toString(16)`, `(42).valueOf()`) is routed
//! into the ambient `class Number` with the primitive BOXED as `this` (shape-based
//! dispatch — the engine resolves `(method, arity)` on the class at compile time,
//! NOT JS prototypes). This reuses the boolean primitive→prelude mechanism. The
//! `.ts` bodies call the PRIVATE `engine.num_*` formatters (the irreducible Rust
//! float→string / radix / toFixed/toPrecision/toExponential — one source of truth).
//!
//! Each test runs a REAL `.ts` program end to end and asserts EXACT captured
//! stdout — the honesty floor (no hardcoding, real output).

use super::assert_stdout;

// ---------------------------------------------------------------------------
// toFixed — fixed-point formatting via the Rust formatter (engine.num_to_fixed).
// ---------------------------------------------------------------------------

#[test]
fn int_to_fixed_two() {
    assert_stdout("console.log((5).toFixed(2));", "5.00\n");
}

#[test]
fn float_to_fixed_two() {
    assert_stdout("console.log((3.14159).toFixed(2));", "3.14\n");
}

#[test]
fn to_fixed_no_arg() {
    // `toFixed()` defaults to 0 digits (the `.ts` default-param `digits = 0`).
    assert_stdout("console.log((3.7).toFixed());", "4\n");
}

// ---------------------------------------------------------------------------
// toString(radix) — radix formatting via the Rust radix formatter.
// ---------------------------------------------------------------------------

#[test]
fn to_string_hex() {
    assert_stdout("console.log((255).toString(16));", "ff\n");
}

#[test]
fn to_string_binary() {
    assert_stdout("console.log((10).toString(2));", "1010\n");
}

#[test]
fn to_string_default_base10() {
    // No radix arg → default 10 (the `.ts` default-param `radix = 10`).
    assert_stdout("console.log((1.5).toString());", "1.5\n");
}

// ---------------------------------------------------------------------------
// toPrecision — significant digits.
// ---------------------------------------------------------------------------

#[test]
fn to_precision_three() {
    assert_stdout("console.log((3.14159).toPrecision(3));", "3.14\n");
}

// ---------------------------------------------------------------------------
// valueOf — the primitive number itself (a pure `.ts` body: `return this`).
// ---------------------------------------------------------------------------

#[test]
fn value_of_int() {
    assert_stdout("console.log((42).valueOf());", "42\n");
}

// ---------------------------------------------------------------------------
// A COMPUTED-number receiver: `(1+2).toFixed(1)` — the receiver is a proven
// numeric expression result, routed exactly like a literal.
// ---------------------------------------------------------------------------

#[test]
fn computed_receiver_to_fixed() {
    assert_stdout("console.log((1 + 2).toFixed(1));", "3.0\n");
}

// ---------------------------------------------------------------------------
// Coercion paths that must KEEP WORKING (regression guard): a number in a
// template / a string `+` uses the runtime ToString/`+` trampolines (unchanged
// by this migration — they do NOT route through the prelude class).
// ---------------------------------------------------------------------------

#[test]
fn number_in_template() {
    assert_stdout("console.log(`v=${42}`);", "v=42\n");
}

#[test]
fn number_in_string_concat() {
    assert_stdout(r#"console.log("n=" + 7);"#, "n=7\n");
}

// ---------------------------------------------------------------------------
// `new Number(x)` WRAPPER — stays the engine's wrapper trampoline (typeof ===
// "object"), NOT the prototype-only prelude class. Regression guard.
// ---------------------------------------------------------------------------

#[test]
fn new_number_is_object() {
    assert_stdout("console.log(typeof new Number(5));", "object\n");
}
