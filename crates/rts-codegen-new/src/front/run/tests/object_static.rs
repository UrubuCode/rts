//! Object static methods: hasOwn, is (SameValue), fromEntries (array source;
//! a Map/iterator source still bails honestly).

use super::{assert_bails, assert_stdout};

#[test]
fn has_own_and_is() {
    assert_stdout(
        "const o={a:1}; console.log(Object.hasOwn(o,\"a\"),Object.hasOwn(o,\"z\"));",
        "true false\n",
    );
    assert_stdout(
        "console.log(Object.is(NaN,NaN),Object.is(0,-0));",
        "true false\n",
    );
}

#[test]
fn from_entries_array_source() {
    assert_stdout(
        "const o=Object.fromEntries([[\"x\",7],[\"y\",8]]); console.log(o.x,o.y);",
        "7 8\n",
    );
}

#[test]
fn from_entries_non_array_bails() {
    // A Map/iterator source needs iteration (a later increment) → honest bail.
    assert_bails(
        "function f(m:any):any{return Object.fromEntries(m);} console.log(f([[\"a\",1]]));",
    );
}
