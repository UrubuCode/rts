//! The PRIMORDIAL `Number.prototype` methods as a pure-Rust
//! `#[rtse::class("Number", value)]` value-class (`rts-primitives/src/number/
//! mod.rs`). A method called on a PRIMITIVE number receiver (`(5).toFixed(2)`,
//! `(255).toString(16)`, `(42).valueOf()`) is routed into the "Number" Registry
//! class with the primitive BOXED as `this` (shape-based dispatch — the engine
//! resolves `(method, arity)` on the class at compile time, NOT JS prototypes).
//! This reuses the Boolean/String value-class mechanism. The formatting bodies
//! compute directly in Rust (`number/format.rs` — the irreducible float→string /
//! radix / toFixed/toPrecision/toExponential logic, one source of truth).
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
// `Number(x)` (no `new`) — `front/run/globals.rs`'s `"Number"` arm delegates to
// the engine's own ToNumber, returning a PRIMITIVE number (typeof === "number").
// JS ToNumber: number→itself, string→parsed, boolean→1/0, null→0.
// ---------------------------------------------------------------------------

#[test]
fn factory_returns_primitive() {
    assert_stdout("console.log(typeof Number(5));", "number\n");
}

#[test]
fn factory_number_identity() {
    assert_stdout("console.log(Number(5));", "5\n");
}

#[test]
fn factory_parses_string() {
    assert_stdout(r#"console.log(Number("42"));"#, "42\n");
}

#[test]
fn factory_boolean_to_one() {
    assert_stdout("console.log(Number(true));", "1\n");
}

#[test]
fn factory_null_to_zero() {
    assert_stdout("console.log(Number(null));", "0\n");
}

// ---------------------------------------------------------------------------
// `new Number(x)` WRAPPER — the value-class ctor (`Entry::Rtse` classed
// "Number", typeof === "object"). Its methods route to the SAME Rust bodies as
// the primitive autobox path (dual `this`), reading the primitive from the
// `prim` slot.
// ---------------------------------------------------------------------------

#[test]
fn new_number_is_object() {
    assert_stdout("console.log(typeof new Number(5));", "object\n");
}

#[test]
fn new_number_instanceof() {
    assert_stdout("console.log(new Number(7) instanceof Number);", "true\n");
}

#[test]
fn wrapper_value_of() {
    assert_stdout("console.log(new Number(7).valueOf());", "7\n");
}

#[test]
fn wrapper_to_fixed() {
    // The wrapper-object receiver reaches the SAME `.ts` `toFixed` as a primitive,
    // unwrapping `this.__prim` (dual-`this` via `__num_val`).
    assert_stdout("console.log(new Number(7).toFixed(2));", "7.00\n");
}

#[test]
fn wrapper_to_string_radix() {
    assert_stdout(r#"console.log(new Number(255).toString(16));"#, "ff\n");
}

#[test]
fn wrapper_from_string_arg() {
    // The ctor runs the engine's own ToNumber, so a string arg is parsed too.
    assert_stdout(r#"console.log(new Number("9").valueOf());"#, "9\n");
}
