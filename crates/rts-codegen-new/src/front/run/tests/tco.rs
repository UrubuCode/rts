//! Tail-call optimization (Phase 4): `return f(args)` to a tail-callable user
//! function lowers to a Cranelift `return_call`, so deep tail recursion runs in
//! CONSTANT stack instead of overflowing.
//!
//! Each test recurses ~1,000,000 deep — far past any thread's stack limit. WITHOUT
//! TCO these stack-overflow (crash); WITH it they run flat and return. So a passing
//! result IS the proof the tail call became a `return_call`.

use super::assert_stdout;

#[test]
fn deep_self_tail_recursion_does_not_overflow() {
    // count down 1,000,000 in tail position, accumulating — flat stack under TCO.
    assert_stdout(
        "function count(n: number, acc: number): number {\n\
         \x20 if (n < 1) return acc;\n\
         \x20 return count(n - 1, acc + 1);\n\
         }\n\
         console.log(count(1000000, 0));",
        "1000000\n",
    );
}

#[test]
fn deep_mutual_tail_recursion_does_not_overflow() {
    // ev/od tail-call each other 1,000,000 deep — exercises CROSS-fn return_call
    // (both must use the `tail` conv). Uses `n < 1` (not `===`) to avoid the
    // HIR equality-ambiguity bail.
    assert_stdout(
        "function ev(n: number): number {\n\
         \x20 if (n < 1) return 1;\n\
         \x20 return od(n - 1);\n\
         }\n\
         function od(n: number): number {\n\
         \x20 if (n < 1) return 0;\n\
         \x20 return ev(n - 1);\n\
         }\n\
         console.log(ev(1000000));",
        "1\n",
    );
}
