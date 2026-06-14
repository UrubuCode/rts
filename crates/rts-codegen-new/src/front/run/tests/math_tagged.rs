//! `Math.*` numeric statics accept TAGGED arguments (an `any`, a polymorphic
//! expression) by coercing them ToNumber, instead of bailing on a non-proven
//! number. Proven-number args still take the existing fast/intrinsic path.

use super::assert_stdout;

#[test]
fn math_min_max_tagged_args() {
    assert_stdout(
        r#"function clamp(x: any, lo: any, hi: any): number {
             return Math.max(lo, Math.min(x, hi));
           }
           console.log(clamp(5, 0, 10), clamp(-3, 0, 10), clamp(15, 0, 10));"#,
        "5 0 10\n",
    );
}

#[test]
fn math_floor_abs_tagged() {
    assert_stdout(
        r#"function f(x: any): number { return Math.floor(x) + Math.abs(x); }
           console.log(f(3.7));"#,
        "6.7\n",
    );
}

#[test]
fn math_min_proven_still_works() {
    assert_stdout(
        r#"console.log(Math.min(3, 1, 2), Math.max(3, 1, 2), Math.floor(2.9));"#,
        "1 3 2\n",
    );
}
