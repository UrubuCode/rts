//! Phase 1 free-function `this` tests: a top-level `function`/function-expression
//! that references `this` COMPILES (gets a synthesized `this` param), a plain call
//! passes `undefined` as the receiver, and a non-`this` function is unchanged.
//! (`new F()` passing a real instance is PHASE 2.)

use super::assert_stdout;

#[test]
fn free_function_this_is_undefined_when_called_plain() {
    assert_stdout(
        r#"function f(): string { return typeof this; }
           console.log(f());"#,
        "undefined\n",
    );
}

#[test]
fn free_function_this_compiles_and_other_args_work() {
    assert_stdout(
        r#"function f(a: number, b: number): number { let t = typeof this; return a + b; }
           console.log(f(2, 3));"#,
        "5\n",
    );
}

#[test]
fn function_without_this_unchanged() {
    assert_stdout(
        r#"function g(x: number): number { return x * 2; }
           console.log(g(21));"#,
        "42\n",
    );
}

#[test]
fn function_expression_this_is_undefined() {
    // `const F = function(){…}` reaches `funcs` as a `HirFunc` too — its `this`
    // must be transformed on the same path as a `function` declaration.
    assert_stdout(
        r#"const f = function(): string { return typeof this; };
           console.log(f());"#,
        "undefined\n",
    );
}
