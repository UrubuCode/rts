//! P4.7: Array methods that take a CALLBACK function value — `map`/`filter`/
//! `forEach`/`find`/`findIndex`/`some`/`every`/`reduce`. Each invokes the
//! callback per element through the codegen-owned `__rtsadp_arr_*` trampolines
//! (which call back into the reified function value via `__rtsadp_fn_invoke`).
//!
//! The callback is a non-capturing inline arrow, extracted by the P4.6 pre-pass
//! into a synthesized top-level function and reified to a `TAG_FUNCTION` value.
//! Capturing callbacks + `.sort(comparator)` BAIL explicitly (the soundness
//! floor) — never a wrong value.

use super::{assert_bails, assert_stdout};

#[test]
fn map_doubles() {
    assert_stdout(
        "let a=[1,2,3]; let b=a.map((x:number)=>x*2); console.log(b.join(\",\"));",
        "2,4,6\n",
    );
}

#[test]
fn filter_gt() {
    assert_stdout(
        "let a=[1,2,3,4]; let b=a.filter((x:number)=>x>2); console.log(b.join(\",\"));",
        "3,4\n",
    );
}

#[test]
fn for_each_prints() {
    // Expression-bodied arrow: `console.log(x)` returns undefined, which forEach
    // discards. (A block-bodied arrow with no `return` currently bails at arrow
    // extraction — "may fall through without returning a value" — a pre-existing
    // P4.6 limitation, not specific to callbacks.)
    assert_stdout(
        "[1,2,3].forEach((x:number)=>console.log(x));",
        "1\n2\n3\n",
    );
}

#[test]
fn reduce_with_init() {
    assert_stdout(
        "let a=[1,2,3,4]; console.log(a.reduce((acc:number,x:number)=>acc+x, 0));",
        "10\n",
    );
}

#[test]
fn reduce_no_init() {
    assert_stdout(
        "let a=[5,6,7]; console.log(a.reduce((acc:number,x:number)=>acc+x));",
        "18\n",
    );
}

#[test]
fn find_first_match() {
    assert_stdout(
        "let a=[1,2,3,4]; console.log(a.find((x:number)=>x>2));",
        "3\n",
    );
}

#[test]
fn find_index_match() {
    assert_stdout(
        "let a=[1,2,3]; console.log(a.findIndex((x:number)=>x===2));",
        "1\n",
    );
}

#[test]
fn some_and_every() {
    assert_stdout(
        "let a=[1,2,3]; console.log(a.some((x:number)=>x>2), a.every((x:number)=>x>0));",
        "true true\n",
    );
}

#[test]
fn index_arg_passed_to_callback() {
    // The callback receives (element, index, array): `x + i` uses the index arg.
    assert_stdout(
        "let a=[10,20]; let b=a.map((x:number,i:number)=>x+i); console.log(b.join(\",\"));",
        "10,21\n",
    );
}

#[test]
fn chained_filter_map() {
    // A chained array-returning method call (`a.filter(..).map(..)`): the inner
    // `.filter(..)` result is itself a proven array receiver for `.map(..)`.
    // (Predicate avoids `%`: float remainder needs runtime fmod — a separate
    // increment — and `number` params are Float64, so `x%2` would bail.)
    assert_stdout(
        "let a=[1,2,3,4]; console.log(a.filter((x:number)=>x>2).map((x:number)=>x*10).join(\",\"));",
        "30,40\n",
    );
}

#[test]
fn map_on_array_literal() {
    // An array literal is a proven array receiver (no intermediate `let`).
    assert_stdout("console.log([1,2,3].map((x:number)=>x+1).join(\",\"));", "2,3,4\n");
}

// ---- Bail tests: capturing callback + comparator sort (soundness floor) ----

#[test]
fn capturing_callback_bails() {
    // The callback captures the outer local `k` — a closure, a later increment.
    // The arrow stays an `Arrow` node (extraction rejected it) → BAIL.
    assert_bails("let k=2; let a=[1,2]; console.log(a.map((x:number)=>x*k).join(\",\"));");
}

#[test]
fn sort_with_comparator_bails() {
    // `.sort(cmp)` is not in the implemented Array surface → BAIL (no Registry
    // row for `Array.sort`).
    assert_bails("let a=[3,1,2]; a.sort((x:number,y:number)=>x-y); console.log(a.join(\",\"));");
}
