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
fn tagged_template_bails() {
    // A TAGGED template (`tag\`…\``) is a separate feature — it must BAIL, never be
    // mistaken for a plain template.
    assert_bails(r#"function tag(s: any): any { return s; } console.log(tag`hi ${1}`);"#);
}
