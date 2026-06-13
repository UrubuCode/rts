//! P4.6: first-class FUNCTION values (non-capturing) with the FIXED uniform
//! 5-slot calling convention.
//!
//! A function used as a VALUE (stored in a `let`, passed as an arg, returned,
//! called through a variable) becomes a `TAG_FUNCTION` PolyValue (a reified
//! `Entry::Function` holding the THUNK address) and is invoked indirectly through
//! `extern "C" fn(a0,a1,a2,a3,rest) -> u64`. Direct monomorphic calls keep the
//! native fast path.
//!
//! What BAILS (explicit `Unsupported`, never a wrong value): a function value
//! that captures an outer local (a closure — a later increment); an async /
//! generator function value.

use super::{assert_bails, assert_stdout};

// ===========================================================================
// Store + call: a function stored in a `let`/`const`, called through it.
// ===========================================================================

#[test]
fn store_and_call() {
    assert_stdout("const f = (x: number) => x * 2; console.log(f(5));", "10\n");
}

#[test]
fn store_two_args() {
    assert_stdout("const add = (a:number,b:number)=>a+b; console.log(add(3,4));", "7\n");
}

// ===========================================================================
// Pass as an argument + call it inside the callee (the inline arrow is a value).
// ===========================================================================

#[test]
fn pass_function_as_arg() {
    assert_stdout(
        "function apply(g, v: number) { return g(v); } console.log(apply((x:number)=>x+1, 9));",
        "10\n",
    );
}

// ===========================================================================
// Return a function (the returned arrow is a value), store it, call it.
// ===========================================================================

#[test]
fn return_a_function() {
    assert_stdout(
        "function mk() { return (x: number) => x*x; } let sq = mk(); console.log(sq(6));",
        "36\n",
    );
}

// ===========================================================================
// typeof a function value → "function".
// ===========================================================================

#[test]
fn typeof_function_value() {
    assert_stdout("const f = ()=>1; console.log(typeof f);", "function\n");
}

#[test]
fn typeof_named_function_value() {
    // A named top-level function referenced as a value (not called) also reifies.
    assert_stdout("function g(x: number){ return x; } console.log(typeof g);", "function\n");
}

// ===========================================================================
// Combinations: a function value passed AND the result used.
// ===========================================================================

#[test]
fn pass_named_function_as_value() {
    // Pass a NAMED top-level function (reified by ident) as a value, invoke it.
    assert_stdout(
        "function twice(n: number){ return n * 2; } function run(g, x: number){ return g(x); } console.log(run(twice, 21));",
        "42\n",
    );
}

#[test]
fn invoke_with_four_args() {
    // Four positional args ride a0..a3 — no rest array allocated.
    assert_stdout(
        "function call4(g, a:number,b:number,c:number,d:number){ return g(a,b,c,d); } const sum = (a:number,b:number,c:number,d:number)=>a+b+c+d; console.log(call4(sum, 1, 2, 3, 4));",
        "10\n",
    );
}

#[test]
fn invoke_with_five_args_uses_rest_array() {
    // FIVE args: the first four ride a0..a3, the fifth rides the `rest` ARRAY (a
    // real `Entry::Vec`, TAG_OBJECT). The 5-param callee reads its 5th param from
    // that rest array via the thunk's VEC_GET — exercising the overflow path of
    // the fixed uniform 4+rest ABI end to end.
    assert_stdout(
        "function call5(g, a:number,b:number,c:number,d:number,e:number){ return g(a,b,c,d,e); } const sum5 = (a:number,b:number,c:number,d:number,e:number)=>a+b+c+d+e; console.log(call5(sum5, 1, 2, 3, 4, 5));",
        "15\n",
    );
}

// ===========================================================================
// Bails: closures + async function values (the soundness floor).
// ===========================================================================

#[test]
fn closure_capturing_local_bails() {
    // `g` captures the outer local `k` — a closure, a later increment. BAIL.
    assert_bails("let k = 3; const g = (x: number) => x + k; console.log(g(1));");
}

#[test]
fn async_function_value_bails() {
    // An async function as a VALUE (it returns a Promise / suspends) bails.
    assert_bails("const f = async () => 1; console.log(typeof f);");
}

#[test]
fn closure_passed_as_arg_bails() {
    // An inline arrow that captures an outer local, passed as an arg, also bails
    // (arrow extraction rejects it; the lowering reports `expression arrow`).
    assert_bails(
        "function apply(g, v: number){ return g(v); } let bonus = 10; console.log(apply((x:number)=>x+bonus, 1));",
    );
}
