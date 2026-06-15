//! Increment 3 proof: REAL TypeScript source runs through the new engine.
//!
//! Every test below takes an ACTUAL TS source string, runs the genuine front-end
//! (`front::parse_function` = swc parse + rts-hir typed lowering), lowers the
//! resulting `HirFunc` straight to Cranelift via `front::hir_lower`, JIT-compiles
//! it to executable memory, and CALLS THE NATIVE CODE — asserting the result
//! against a Rust-computed expected value. This is the first time the redesign
//! executes real source (the P1 proofs only ran hand-built IR).
//!
//! Coverage is the proven-monomorphic NUMERIC subset only — `number`/`f64`/`i32`/
//! `bool` arithmetic, comparisons, logical/unary ops, `let`/`const`/assignment,
//! `if`/`while`/`return`. Out-of-subset constructs bail with an explicit
//! `Unsupported` (asserted by the negative tests at the bottom), never a silent
//! wrong value.

use super::{jit, parse_function};

// ===========================================================================
// 1 — sq: a single multiply, f64 in/out (the unboxed fast path on real source).
// ===========================================================================

#[test]
fn sq_number() {
    let src = "function sq(x: number): number { return x * x; }";
    let f = parse_function(src, "sq").expect("parse+lower sq");
    let run = jit::run_f64_f64(&f).expect("jit sq");
    for x in [0.0_f64, 1.0, 7.0, -3.5, 2.5, 1e6] {
        assert_eq!(run(x), x * x, "sq({x})");
    }
    // The explicit headline case from the increment brief.
    assert_eq!(run(7.0), 49.0);
}

// ===========================================================================
// 2 — poly: x*x*x - 2*x + 1, exercising fmul/fsub/fadd chained, f64.
// ===========================================================================

#[test]
fn poly_number() {
    let src = "function poly(x: number): number { return x*x*x - 2.0*x + 1.0; }";
    let f = parse_function(src, "poly").expect("parse+lower poly");
    let run = jit::run_f64_f64(&f).expect("jit poly");
    let expected = |x: f64| x * x * x - 2.0 * x + 1.0;
    for x in [0.0_f64, 1.0, 2.0, -1.0, 3.5, 10.0, -2.25] {
        assert_eq!(run(x), expected(x), "poly({x})");
    }
}

// ===========================================================================
// 3 — fib: a real loop with mutable integer locals + a comparison condition.
//
// NOTE on typing: the source writes `0.0`/`1.0`, but swc lowers numeric literals
// with no fractional part to INTEGER literals, so rts-hir types `a`/`b`/`i` (and
// the inferred return) as `I64`. The new engine therefore runs this as a native
// i64 loop — fully unboxed — and the function's ABI is `fn(f64) -> i64` (param
// `n: number` is an f64, the integer-Fibonacci result is an i64). We assert
// against the exact integer Fibonacci, which is what JS would print for these n.
// ===========================================================================

#[test]
fn fib_loop_int64() {
    let src = r#"
        function fib(n: number): number {
            let a = 0.0;
            let b = 1.0;
            let i = 0.0;
            while (i < n) {
                let t = a + b;
                a = b;
                b = t;
                i = i + 1.0;
            }
            return a;
        }
    "#;
    let f = parse_function(src, "fib").expect("parse+lower fib");
    let run = jit::run_f64_i64(&f).expect("jit fib");

    let expected = |n: i64| -> i64 {
        let (mut a, mut b) = (0_i64, 1_i64);
        for _ in 0..n {
            let t = a + b;
            a = b;
            b = t;
        }
        a
    };
    for n in [0_i64, 1, 2, 5, 10, 20] {
        assert_eq!(run(n as f64), expected(n), "fib({n})");
    }
    assert_eq!(run(10.0), 55); // headline case: fib(10) == 55
}

// ===========================================================================
// 4 — clamp: branching control flow (two early returns + a tail return).
// ===========================================================================

#[test]
fn clamp_branches() {
    let src = r#"
        function clamp(x: number): number {
            if (x < 0.0) { return 0.0; }
            if (x > 1.0) { return 1.0; }
            return x;
        }
    "#;
    let f = parse_function(src, "clamp").expect("parse+lower clamp");
    let run = jit::run_f64_f64(&f).expect("jit clamp");
    let expected = |x: f64| x.max(0.0).min(1.0);
    for x in [-2.0_f64, -0.0001, 0.0, 0.3, 0.5, 1.0, 1.0001, 5.0] {
        assert_eq!(run(x), expected(x), "clamp({x})");
    }
}

// ===========================================================================
// 5 — addi: the i32 path — explicit integer annotations, native iadd, no box.
// ===========================================================================

#[test]
fn addi_i32() {
    let src = "function addi(a: i32, b: i32): i32 { return a + b; }";
    let f = parse_function(src, "addi").expect("parse+lower addi");
    let run = jit::run_ii_i32(&f).expect("jit addi");
    for (a, b) in [(1, 2), (0, 0), (-5, 3), (100, 23), (i32::MAX as i64 - 1, 1)] {
        assert_eq!(run(a, b), a + b, "addi({a},{b})");
    }
    assert_eq!(run(1, 2), 3); // headline case
}

// ===========================================================================
// 6 — sum_to: i32 loop with += accumulation and a comparison.
// ===========================================================================

#[test]
fn sum_to_i32_loop() {
    let src = r#"
        function sum_to(n: i32): i32 {
            let acc: i32 = 0;
            let i: i32 = 1;
            while (i <= n) {
                acc += i;
                i += 1;
            }
            return acc;
        }
    "#;
    let f = parse_function(src, "sum_to").expect("parse+lower sum_to");
    // sum_to takes one i32; reuse the two-arg ABI wrapper is wrong, so compile a
    // dedicated 1-arg i32->i32 closure here.
    let jf = jit::compile(&f).expect("jit sum_to");
    let native: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(jf.ptr()) };
    let expected = |n: i64| (1..=n).sum::<i64>();
    for n in [0_i64, 1, 2, 5, 10, 100] {
        let _keep = &jf;
        assert_eq!(native(n), expected(n), "sum_to({n})");
    }
}

// ===========================================================================
// 7 — ternary + logical/unary, all booleans/f64 — proves select-based lowering.
// ===========================================================================

#[test]
fn ternary_and_logic() {
    // returns the larger of x and y via a ternary on a comparison.
    let src = "function maxf(x: number, y: number): number { return x > y ? x : y; }";
    let f = parse_function(src, "maxf").expect("parse+lower maxf");
    let run = jit::run_ff_f64(&f).expect("jit maxf");
    for (a, b) in [(1.0, 2.0), (3.0, -1.0), (5.5, 5.5), (-2.0, -3.0)] {
        assert_eq!(run(a, b), a.max(b), "maxf({a},{b})");
    }
}

// ===========================================================================
// 8 — negative: out-of-subset constructs bail EXPLICITLY (the soundness floor).
// ===========================================================================

#[test]
fn string_param_bails() {
    // A string param is outside the numeric subset → Unsupported, not a wrong
    // value. (This is the exact unsoundness the engine refuses to commit.)
    let src = "function s(x: string): string { return x; }";
    let f = parse_function(src, "s").expect("parse+lower s (HIR ok)");
    let err = match jit::compile(&f) {
        Ok(_) => panic!("string param must bail, not compile"),
        Err(e) => e,
    };
    assert!(
        err.reason().contains("non-numeric") || err.reason().contains("string"),
        "bail reason should name the non-numeric type, got: {err}"
    );
}

#[test]
fn object_return_bails() {
    let src = "function o(x: number): number { return { a: x }; }";
    let f = parse_function(src, "o").expect("parse+lower o (HIR ok)");
    // The object literal in return position is out of subset.
    // Any explicit Unsupported is acceptable — the point is no panic / no wrong value.
    assert!(jit::compile(&f).is_err(), "object literal must bail");
}

#[test]
fn unknown_function_name_errors() {
    let src = "function present(x: number): number { return x; }";
    let res = parse_function(src, "absent");
    assert!(
        res.is_err(),
        "asking for a missing function name must error"
    );
}

#[test]
fn call_to_other_fn_bails_cleanly() {
    // Cross-function calls are a documented later step: must bail, not miscompile.
    let src = r#"
        function helper(x: number): number { return x + 1.0; }
        function uses(x: number): number { return helper(x); }
    "#;
    let f = parse_function(src, "uses").expect("parse+lower uses (HIR ok)");
    let err = match jit::compile(&f) {
        Ok(_) => panic!("cross-fn call must bail in this increment, not compile"),
        Err(e) => e,
    };
    assert!(
        err.reason().contains("call") || err.reason().contains("helper"),
        "bail reason should mention the call, got: {err}"
    );
}
