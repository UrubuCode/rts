//! `++` / `--` (prefix and postfix) on a `Tagged`-repr local. A counter that
//! became `Tagged` (e.g. bound from an `any` value) must still increment /
//! decrement with correct JS semantics (postfix yields the OLD number, prefix
//! the NEW one) instead of bailing.

use super::assert_stdout;

#[test]
fn postfix_inc_on_tagged() {
    // `x` is Tagged (an `any` param), then `r = x`; `r++` must work.
    assert_stdout(
        r#"function f(x: any): number { let r = x; r++; return r; }
           console.log(f(5));"#,
        "6\n",
    );
}

#[test]
fn prefix_and_postfix_values() {
    assert_stdout(
        r#"function g(x: any): void {
             let i = x;
             console.log(i++);
             console.log(i);
             console.log(++i);
           }
           g(10);"#,
        "10\n11\n12\n",
    );
}

#[test]
fn dec_on_tagged_loop() {
    assert_stdout(
        r#"function h(n: any): number {
             let i = n; let sum = 0;
             while (i > 0) { sum += i; i--; }
             return sum;
           }
           console.log(h(4));"#,
        "10\n",
    );
}
