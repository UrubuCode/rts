//! Builtin behavior regression tests: Array mutation/variadic ops, Object statics,
//! String Unicode methods, Error cause, JSON reviver, boolean ToNumber, and `-0`.
//! Each block covers the normal path plus a variation (chained, nested, edge value).

use super::{assert_bails, assert_stdout};

// ── Array.splice (mutating: remove / insert / replace) ──────────────────────────

#[test]
fn splice_remove_insert_replace() {
    assert_stdout(
        "let a=[1,2,3,4,5]; let r=a.splice(2,2); console.log(a.join(),r.join());",
        "1,2,5 3,4\n",
    );
    assert_stdout("let a=[1,2,5]; a.splice(2,0,3,4); console.log(a.join());", "1,2,3,4,5\n");
    assert_stdout("let a=[1,2,3,4]; a.splice(1,2,8,9); console.log(a.join());", "1,8,9,4\n");
}

#[test]
fn splice_chained_result_is_array() {
    // The removed-elements result chains as an array (`.join` on it).
    assert_stdout("console.log([1,2,3,4].splice(1,2).join());", "2,3\n");
}

// ── Array.toSpliced (non-mutating) ──────────────────────────────────────────────

#[test]
fn to_spliced_keeps_receiver() {
    assert_stdout(
        "let a=[1,2,3,4]; let b=a.toSpliced(1,2,9); console.log(b.join(),a.join());",
        "1,9,4 1,2,3,4\n",
    );
}

// ── Array.fill range + copyWithin(target) ───────────────────────────────────────

#[test]
fn fill_range_and_copy_within() {
    assert_stdout("let a=[1,1,1,1]; a.fill(9,2); console.log(a.join());", "1,1,9,9\n");
    assert_stdout("let a=[1,1,1,1]; a.fill(7,1,3); console.log(a.join());", "1,7,7,1\n");
    assert_stdout("let a=[1,2,3,4,5]; a.copyWithin(-2); console.log(a.join());", "1,2,3,1,2\n");
}

// ── Array.push / unshift / concat (variadic) ────────────────────────────────────

#[test]
fn variadic_push_unshift_concat() {
    assert_stdout("let a=[1]; a.push(2,3,4); console.log(a.join());", "1,2,3,4\n");
    assert_stdout("let a=[3,4]; a.unshift(1,2); console.log(a.join());", "1,2,3,4\n");
    assert_stdout("console.log([1,2].concat([3],[4,5]).join());", "1,2,3,4,5\n");
    assert_stdout("console.log([1].concat(2,3).join());", "1,2,3\n");
}

// ── Object.hasOwn / Object.is ───────────────────────────────────────────────────

#[test]
fn object_has_own_and_is() {
    assert_stdout(
        "const o={a:1}; console.log(Object.hasOwn(o,\"a\"),Object.hasOwn(o,\"z\"));",
        "true false\n",
    );
    assert_stdout("console.log(Object.is(NaN,NaN),Object.is(0,-0));", "true false\n");
}

// ── String.normalize / isWellFormed / toWellFormed ──────────────────────────────

#[test]
fn string_unicode_methods() {
    // NFD decomposes "é" into base + combining mark → 2 code units.
    assert_stdout("console.log(\"é\".normalize(\"NFD\").length);", "2\n");
    assert_stdout("console.log(\"abc\".isWellFormed());", "true\n");
    assert_stdout("console.log(\"abc\".toWellFormed());", "abc\n");
}

// ── Error(message, { cause }) ───────────────────────────────────────────────────

#[test]
fn error_with_cause_option() {
    assert_stdout(
        "const e=new Error(\"outer\",{cause:new Error(\"inner\")}); console.log(e.message,e.cause.message);",
        "outer inner\n",
    );
    // The 1-arg form leaves cause undefined.
    assert_stdout("const e=new Error(\"x\"); console.log(e.cause===undefined);", "true\n");
}

// ── JSON.parse reviver (transform + delete) ─────────────────────────────────────

#[test]
fn json_parse_reviver_transform() {
    assert_stdout(
        "const o=JSON.parse(\"{\\\"a\\\":1,\\\"b\\\":2}\",(k:string,v:any)=>typeof v===\"number\"?v*2:v); console.log(o.a,o.b);",
        "2 4\n",
    );
}

#[test]
fn json_parse_reviver_delete() {
    assert_stdout(
        "const o=JSON.parse(\"{\\\"a\\\":1,\\\"c\\\":2}\",(k:string,v:any)=>k===\"c\"?undefined:v); console.log(o.a,o.c===undefined);",
        "1 true\n",
    );
}

// ── ToNumber of boolean in arithmetic / unary ───────────────────────────────────

#[test]
fn boolean_tonumber_arith() {
    assert_stdout("console.log(true+1,false+5,true*3+false);", "2 5 3\n");
    assert_stdout("console.log(-true,~true,~1.5);", "-1 -2 -2\n");
}

// ── -0 literal preserves IEEE-754 negative zero ─────────────────────────────────

#[test]
fn negative_zero_literal() {
    assert_stdout("console.log(1/-0);", "-Infinity\n");
    assert_stdout("console.log(Object.is(0,-0),(-0===0));", "false true\n");
}

// ── Object.fromEntries from array (Map source still bails) ───────────────────────

#[test]
fn from_entries_array_and_non_array_bail() {
    assert_stdout(
        "const o=Object.fromEntries([[\"x\",7],[\"y\",8]]); console.log(o.x,o.y);",
        "7 8\n",
    );
    assert_bails("function f(m:any):any{return Object.fromEntries(m);} console.log(f([[\"a\",1]]));");
}
