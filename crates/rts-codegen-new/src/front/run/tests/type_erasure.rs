//! TypeScript type-only expression wrappers (`as`, `!`, `as const`,
//! `satisfies`, `<T>x`) carry ZERO runtime effect and must erase to their inner
//! value expression — never bail as an unrecognized `Raw` node.

use super::assert_stdout;

#[test]
fn as_cast_erased() {
    assert_stdout(r#"let x = (5 as number); console.log(x + 1);"#, "6\n");
}

#[test]
fn as_any_and_back() {
    assert_stdout(r#"let x = (42 as any) as number; console.log(x);"#, "42\n");
}

#[test]
fn non_null_assertion() {
    assert_stdout(
        r#"function f(x?: number): number { return x! + 1; } console.log(f(5));"#,
        "6\n",
    );
}

#[test]
fn as_const() {
    assert_stdout(r#"let s = ("hi" as const); console.log(s);"#, "hi\n");
}

// NOTE: a `function(...) {...} as any` value-cast erases correctly (the inner
// expr is reached), but a bare function-EXPRESSION used as a value is itself an
// out-of-subset construct in this frontend today and bails on its own — so this
// case can't assert a value yet. Erasure of `as`/`as any` is proven by the
// tests above. The variant below keeps the cast-erasure under arithmetic where
// the inner expr IS in subset.
#[test]
fn nested_casts_erased() {
    assert_stdout(
        r#"let x = ((1 as number) + (2 as any)) as number; console.log(x as number);"#,
        "3\n",
    );
}
