//! P5.15: OBJECT-LITERAL METHODS — `{ field, method() {…} }`.
//!
//! A method-bearing object literal synthesizes a "literal class" (its methods
//! become `this`-first functions); `obj.method(args)` static-dispatches and
//! `toString`/`valueOf` make object-literal ToPrimitive (P5.14) work.

use super::{assert_bails, assert_stdout};

#[test]
fn method_uses_field() {
    assert_stdout(
        r#"let o = { name: "rts", greet() { return "hi " + this.name; } }; console.log(o.greet());"#,
        "hi rts\n",
    );
}

#[test]
fn method_field_and_arg() {
    assert_stdout(
        "let acc = { total: 0, add(n: number) { this.total = this.total + n; return this.total; } }; acc.add(3); console.log(acc.add(4));",
        "7\n",
    );
}

#[test]
fn value_of_in_arithmetic() {
    assert_stdout(
        "let o = { v: 10, valueOf() { return this.v; } }; console.log(o + 5);",
        "15\n",
    );
}

#[test]
fn to_string_in_template() {
    assert_stdout(
        r#"let p = { x: 2, toString() { return "P" + this.x; } }; console.log(`${p}`);"#,
        "P2\n",
    );
}

#[test]
fn method_calling_method() {
    assert_stdout(
        "let o = { n: 5, dbl() { return this.n * 2; }, quad() { return this.dbl() * 2; } }; console.log(o.quad());",
        "20\n",
    );
}

#[test]
fn console_log_shows_fields_and_methods() {
    // A literal METHOD is an own enumerable property (bun shows it as
    // `f: [Function: f]`); the engine's inspect renders a function slot as
    // `f: function` — a known FORMAT divergence for function values, not a
    // wrong value (the field/method set matches bun's).
    assert_stdout(
        "let o = { a: 1, f() { return 2; } }; console.log(o);",
        "{ a: 1, f: function }\n",
    );
}

#[test]
fn string_of_obj_literal() {
    assert_stdout(
        r#"let o = { toString() { return "custom"; } }; console.log(String(o));"#,
        "custom\n",
    );
}

#[test]
fn value_of_only_no_fields() {
    // A valueOf-only literal with no fields: `+` uses the default hint → valueOf.
    assert_stdout(
        "let o = { valueOf() { return 42; } }; console.log(o + 1);",
        "43\n",
    );
}

#[test]
fn two_distinct_literals_share_or_separate() {
    // Two DIFFERENT method-bearing literals (different bodies) each dispatch their
    // own method — the content-keyed literal classes must not cross-wire.
    assert_stdout(
        "let a = { v: 1, get2() { return this.v + 1; } }; let b = { v: 10, get2() { return this.v + 100; } }; console.log(a.get2() + b.get2());",
        "112\n",
    );
}

#[test]
fn method_no_args_returns_string_literal() {
    assert_stdout(
        r#"let o = { tag() { return "T"; } }; console.log(o.tag());"#,
        "T\n",
    );
}

// ===========================================================================
// Bails — out of subset (sound: the literal keeps no class, the call bails).
// ===========================================================================

#[test]
fn bail_computed_method_name() {
    assert_bails(r#"let k = "m"; let o = { a: 1, [k]() { return 2; } }; console.log(o.m());"#);
}

#[test]
fn bail_generator_method() {
    assert_bails("let o = { *gen() { yield 1; } }; console.log(o.gen());");
}

#[test]
fn bail_async_method() {
    assert_bails("let o = { async run() { return 1; } }; console.log(o.run());");
}

#[test]
fn literal_getter_reads() {
    // A literal `get x()` is recovered as a literal-class accessor — the
    // property READ runs the getter (bun: 1).
    assert_stdout("let o = { get x() { return 1; } }; console.log(o.x);", "1\n");
}
