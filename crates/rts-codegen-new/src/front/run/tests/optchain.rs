//! Optional-chaining tests (P5.8) — `a?.b`, `a?.[k]`, `a?.f?.()` run end to end
//! with EXACT stdout, with correct nullish short-circuit, plus the bails for the
//! forms we do not desugar soundly.
//!
//! Reads route through the nullish-tolerant `__rtsadp_obj_get` (undefined for a
//! nullish OR non-object receiver), so a nullish at any link short-circuits the
//! whole chain to `undefined`. Optional calls guard the receiver and invoke only
//! when present.

use super::{assert_bails, assert_stdout};

#[test]
fn present_property() {
    assert_stdout(r#"let o = {x: 7}; console.log(o?.x);"#, "7\n");
}

#[test]
fn nested_present() {
    assert_stdout(r#"let o = {a: {b: 5}}; console.log(o?.a?.b);"#, "5\n");
}

#[test]
fn null_receiver_short_circuits() {
    assert_stdout(r#"let o = null; console.log(o?.x);"#, "undefined\n");
}

#[test]
fn undefined_receiver_chain_short_circuits() {
    // A nullish at the FIRST link makes the WHOLE chain undefined — the later
    // `?.bar` is never a real access (opt_get sees undefined and yields undefined).
    assert_stdout(
        r#"let u = undefined; console.log(u?.foo?.bar);"#,
        "undefined\n",
    );
}

#[test]
fn missing_then_optional_short_circuits() {
    // `o.a` is undefined; `o?.a?.b` short-circuits at the second link to undefined.
    assert_stdout(r#"let o = {a: null}; console.log(o?.a?.b);"#, "undefined\n");
}

#[test]
fn computed_optional_index() {
    assert_stdout(r#"let o = {k: 3}; console.log(o?.["k"]);"#, "3\n");
}

#[test]
fn optional_call_present() {
    assert_stdout(r#"let o = {f: () => 42}; console.log(o?.f?.());"#, "42\n");
}

#[test]
fn optional_call_short_circuits() {
    // `o.f` is undefined; the `?.()` link short-circuits to undefined (no invoke).
    assert_stdout(r#"let o = {x: 1}; console.log(o?.f?.());"#, "undefined\n");
}

#[test]
fn optional_in_user_function() {
    assert_stdout(
        r#"function get(o: any): any { return o?.v; }
           let o = {v: 11}; console.log(get(o));"#,
        "11\n",
    );
}

// ---------------------------------------------------------------------------
// Bails: the forms we do not desugar soundly stay `Raw` → explicit Unsupported.
// ---------------------------------------------------------------------------

#[test]
fn non_optional_after_optional_short_circuits() {
    // `a?.b.c` — a NON-optional `.c` after the optional `?.b` still short-circuits
    // the whole chain on a nullish root (`opt_get(undefined, "c")` is `undefined`),
    // and reads through normally when present (o.a.b = 1, `.c` on 1 is `undefined`).
    assert_stdout(
        r#"let o = {a: {b: 1}}; console.log(o?.a.b.c);"#,
        "undefined\n",
    );
    assert_stdout(
        r#"let o: any = null; console.log(o?.a.b.c);"#,
        "undefined\n",
    );
}

#[test]
fn optional_call_spread_bails() {
    assert_bails(r#"let xs = [1]; let o = {f: (a: number) => a}; console.log(o?.f?.(...xs));"#);
}
