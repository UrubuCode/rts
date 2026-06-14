//! TS AMBIENT declarations (`declare ...`) are type-only and must emit NO code.
//! Before the fix a `declare global { function String(...): string; }` reached
//! codegen as a bodyless `__ns_global_String` and bailed with "may fall through
//! without returning a value". These tests pin that ambient declarations are
//! dropped at parse-lowering, so the surrounding real code runs untouched.

use super::assert_stdout;

#[test]
fn declare_function_ignored() {
    assert_stdout(
        r#"declare function nativeThing(x: number): string;
           console.log(1 + 1);"#,
        "2\n",
    );
}

#[test]
fn declare_global_block_ignored() {
    assert_stdout(
        r#"declare global {
             function String(value?: any): string;
             var foo: number;
           }
           console.log("ok");"#,
        "ok\n",
    );
}

#[test]
fn declare_var_ignored() {
    assert_stdout(
        r#"declare var someGlobal: number;
           let x = 5;
           console.log(x);"#,
        "5\n",
    );
}
