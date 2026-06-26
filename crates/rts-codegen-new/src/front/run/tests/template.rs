//! Template-literal tests (P5.8) — `` `a${x}b` `` runs end to end with EXACT
//! stdout, plus the tagged-template bail.
//!
//! Templates desugar to a string-seeded `+` chain that reuses the one
//! string-coercing `__rtsadp_add` ToString path, so every interpolation coerces
//! exactly like JS (`${5}`→"5", `${true}`→"true", `${[1,2,3]}`→"1,2,3",
//! `${null}`→"null", `${undefined}`→"undefined").

use super::{assert_bails, assert_stdout};

#[test]
fn interp_number_var() {
    assert_stdout(r#"let n = 5; console.log(`n=${n}`);"#, "n=5\n");
}

#[test]
fn two_string_vars() {
    assert_stdout(
        r#"let a = "x"; let b = "y"; console.log(`${a}-${b}!`);"#,
        "x-y!\n",
    );
}

#[test]
fn interp_expression() {
    assert_stdout(r#"console.log(`sum=${2+3}`);"#, "sum=5\n");
}

#[test]
fn bool_and_array_coercion() {
    assert_stdout(
        r#"let t = true; console.log(`v=${t} arr=${[1,2,3]}`);"#,
        "v=true arr=1,2,3\n",
    );
}

#[test]
fn null_and_undefined_coercion() {
    assert_stdout(
        r#"let a = null; let b = undefined; console.log(`${a},${b}`);"#,
        "null,undefined\n",
    );
}

#[test]
fn no_interpolation() {
    assert_stdout(r#"console.log(`plain`);"#, "plain\n");
}

#[test]
fn empty_template() {
    assert_stdout(r#"console.log(`${""}` + `end`);"#, "end\n");
}

#[test]
fn leading_and_trailing_text() {
    assert_stdout(r#"let x = 7; console.log(`a${x}b${x}c`);"#, "a7b7c\n");
}

#[test]
fn template_in_user_function() {
    // A template inside a plain user-function body (verifies the per-unit pairing
    // pairs the FunctionDecl source with its lowered HirFunc).
    assert_stdout(
        r#"function g(x: number): string { return `val=${x}`; }
           console.log(g(9));"#,
        "val=9\n",
    );
}

#[test]
fn nested_template() {
    // A template interpolated inside another template's interpolation.
    assert_stdout(
        r#"let n = 2; console.log(`outer ${`in${n}`} done`);"#,
        "outer in2 done\n",
    );
}

#[test]
fn tagged_template_sums_interpolations() {
    // `` tag`x${10}y${20}z` `` → `tag(["x","y","z"], 10, 20)`. The tag fn receives
    // the cooked string-parts array first, then each interpolated value.
    assert_stdout(
        "function sum(strings: string[], a: number, b: number): number { return a + b; } \
         console.log(sum`x${10}y${20}z`);",
        "30\n",
    );
}

#[test]
fn tagged_template_zero_interpolations() {
    // No interpolations: the tag gets only the one-element string-parts array.
    assert_stdout(
        "function f(strings: string[]): number { return 99; } \
         console.log(f`only static text`);",
        "99\n",
    );
}

#[test]
fn tagged_template_nested_in_outer_template() {
    // A tagged template INSIDE an outer template interpolation (`` `${tag`…`}` ``)
    // — the inner one is rebuilt directly during the outer's recovery (otherwise it
    // would surface a `Raw` the cursor never re-reaches).
    assert_stdout(
        "function sum(strings: string[], a: number, b: number): number { return a + b; } \
         console.log(`r=${sum`${5}+${7}`}`);",
        "r=12\n",
    );
}

#[test]
fn tagged_template_extra_args_to_fixed_arity_bails() {
    // A tag fn declaring FEWER params than the call passes interpolations: the
    // engine's user-call enforces exact arity (JS would ignore the extras) — a
    // SOUND bail, never a wrong value. (Call-arity flexibility is a later increment.)
    assert_bails(
        "function partsCount(strings: string[]): number { return strings.length; } \
         console.log(partsCount`a${1}b${2}c`);",
    );
}
