//! Array CALLBACK methods on an UNPROVEN receiver (P5.9 extension).
//!
//! An `any`-typed param, a call return, or a re-`let` local is a Tagged value of
//! unproven class. The proven-array path ([`super::super::method_array`]) does not
//! match it, so `.map`/`.filter`/`.reduce`/`.find`/`.some`/`.every`/`.findIndex`
//! with a NON-CAPTURING callback used to bail "receiver class not statically
//! dispatchable". The dynamic array-callback path lifts that: it dispatches
//! through the same `__rtsadp_arr_*` trampolines (SAFE on a non-array word — they
//! do a HandleTable lookup and see length 0 for a non-Vec entry).
//!
//! NB: `r[0]` indexing on a function-RETURN value is a separate dynamic-index-on-
//! Tagged limitation (returns `undefined` even without a callback), so these tests
//! observe the result via `.join(",")` / a scalar return, not element indexing.
//! A CAPTURING callback (`total = total + x`) is #195 (mutable closures) and still
//! bails — out of scope here.

use super::assert_stdout;

#[test]
fn map_on_array_param() {
    // `.map` on an `any`-typed param; observe via the chained `.join`.
    assert_stdout(
        r#"function doubleAll(xs: any): any { return xs.map((x: number) => x * 2); }
           console.log(doubleAll([1, 2, 3]).join(","));"#,
        "2,4,6\n",
    );
}

#[test]
fn filter_and_reduce_on_param() {
    assert_stdout(
        r#"function sumEvens(xs: any): number {
             return xs.filter((x: number) => x % 2 === 0).reduce((a: number, b: number) => a + b, 0);
           }
           console.log(sumEvens([1, 2, 3, 4, 5, 6]));"#,
        "12\n",
    );
}

#[test]
fn map_on_relet_local() {
    // A re-`let` local bound from a call return is Tagged/unproven; `.map` on it
    // dispatches dynamically. Observe via the chained `.join`.
    assert_stdout(
        r#"function getArr(): any { return [1, 2, 3]; }
           let a = getArr();
           console.log(a.map((x: number) => x * 2).join(","));"#,
        "2,4,6\n",
    );
}

#[test]
fn find_some_every_findindex_on_param() {
    // The predicate family on an unproven receiver, each returning a scalar.
    assert_stdout(
        r#"function firstBig(xs: any): any { return xs.find((x: number) => x > 2); }
           function anyBig(xs: any): boolean { return xs.some((x: number) => x > 3); }
           function allPos(xs: any): boolean { return xs.every((x: number) => x > 0); }
           function idx(xs: any): number { return xs.findIndex((x: number) => x === 3); }
           console.log(firstBig([1, 2, 3, 4]));
           console.log(anyBig([1, 2, 3, 4]));
           console.log(allPos([1, 2, 3]));
           console.log(idx([1, 2, 3, 4]));"#,
        "3\ntrue\ntrue\n2\n",
    );
}

#[test]
fn map_on_any_non_array_is_safe_empty() {
    // SAFETY: `.map` on a genuinely UNKNOWN-kind Tagged receiver (a param whose
    // value the engine cannot prove array/string/number) must not fault — the
    // trampoline does a HandleTable lookup and sees length 0, so the result is an
    // empty array. (A receiver PROVEN string/number routes to its prelude class and
    // bails on the missing `.map`, which is JS-correct — see `map_on_proven_string_bails`.)
    assert_stdout(
        r#"function f(s: any): string { return s.map((x: number) => x * 2).join(","); }
           console.log(f("not an array"));"#,
        "\n",
    );
}

#[test]
fn map_on_proven_string_bails() {
    // A var initialized with a string is PROVEN a string (string_locals), so
    // `s.map(...)` dispatches on `class String` — which has no `.map`. JS throws a
    // TypeError here; the engine BAILS (honesty floor: no wrong-but-safe empty).
    super::assert_bails(
        r#"let s = "not an array";
           console.log(s.map((x: number) => x * 2).join(","));"#,
    );
}
