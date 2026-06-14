//! Phase 4: DATA PROPERTIES on a FUNCTION value (`F.foo = v` / `F.foo`).
//!
//! A function used as a value can carry assigned data properties (statics on a
//! dual-callable function, the array.ts pattern). The property is recorded in a
//! runtime side-table keyed by the function's stable thunk identity; an absent
//! property reads `undefined`.

use super::assert_stdout;

#[test]
fn function_static_property() {
    assert_stdout(
        r#"function F(): number { return 1; }
           F.answer = 42;
           console.log(F.answer);"#,
        "42\n",
    );
}

#[test]
fn function_static_property_on_fn_ctor() {
    // A `this`-using dual-callable function (an fn-ctor) carries a data property.
    // NOTE: calling a function VALUE stored in a property (`F.make(21)`) is a
    // member-call on a function value — a DEEPER gap in the call path, not the
    // property path — so this asserts the data-property core only (the string
    // value round-trips through the side-table keyed by the fn-ctor's identity).
    assert_stdout(
        r#"const F = function(this: any): string {
             if (this instanceof F) { return "new"; }
             return "call";
           };
           F.tag = "x";
           console.log(F.tag);
           console.log(F());"#,
        "x\ncall\n",
    );
}

#[test]
fn function_prop_absent_is_undefined() {
    assert_stdout(
        r#"function F(): number { return 1; }
           console.log(F.nope);"#,
        "undefined\n",
    );
}

#[test]
fn call_function_valued_property() {
    assert_stdout(
        r#"function F(): number { return 1; }
           F.make = function(x: number): number { return x * 2; };
           console.log(F.make(21));"#,
        "42\n",
    );
}

#[test]
fn static_method_on_dual_callable() {
    assert_stdout(
        r#"const S = function(this: any, v: any): any {
             if (this instanceof S) { return v; }
             return v;
           };
           S.fromCharCode = function(code: number): number { return code + 1; };
           console.log(S.fromCharCode(64));"#,
        "65\n",
    );
}

#[test]
fn function_property_function_with_args() {
    assert_stdout(
        r#"function F(): number { return 0; }
           F.add = function(a: number, b: number): number { return a + b; };
           console.log(F.add(20, 22));"#,
        "42\n",
    );
}
