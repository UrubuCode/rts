//! P4.5: Array instance methods WITHOUT callbacks — `recv.method(args)` over the
//! engine's own array representation (a real `Entry::Vec` of boxed PolyValue
//! words), via the codegen-owned `__rtsadp_arr_*` trampolines.

use super::{assert_bails, assert_stdout};

#[test]
fn array_index_of() {
    assert_stdout("let a = [10, 20, 30]; console.log(a.indexOf(20));", "1\n");
    assert_stdout("let a = [10, 20, 30]; console.log(a.indexOf(99));", "-1\n");
}

#[test]
fn array_includes() {
    assert_stdout(
        "let a = [1, 2, 3]; console.log(a.includes(2), a.includes(9));",
        "true false\n",
    );
}

#[test]
fn array_at() {
    assert_stdout(
        "let a = [5, 6, 7]; console.log(a.at(0), a.at(-1));",
        "5 7\n",
    );
}

#[test]
fn array_join() {
    assert_stdout(
        r#"let a = ["x", "y", "z"]; console.log(a.join("-"));"#,
        "x-y-z\n",
    );
    assert_stdout(r#"let a = [1, 2, 3]; console.log(a.join(""));"#, "123\n");
}

#[test]
fn array_push() {
    assert_stdout(
        "let a = [1, 2]; console.log(a.push(3)); console.log(a.length);",
        "3\n3\n",
    );
}

#[test]
fn array_pop() {
    assert_stdout(
        "let a = [1, 2, 3]; console.log(a.pop()); console.log(a.length);",
        "3\n2\n",
    );
}

#[test]
fn array_slice() {
    assert_stdout(
        "let a = [1, 2, 3, 4]; let b = a.slice(1, 3); console.log(b.length, b.at(0));",
        "2 2\n",
    );
}

#[test]
fn array_index_of_heterogeneous() {
    assert_stdout(
        r#"let m = [1, "two", 3]; console.log(m.indexOf("two"));"#,
        "1\n",
    );
}

// ---- P5.2 non-callback methods ----

#[test]
fn array_reverse() {
    assert_stdout(
        "let a = [1, 2, 3]; console.log(a.reverse().join(\",\"));",
        "3,2,1\n",
    );
}

#[test]
fn array_concat() {
    assert_stdout("console.log([1,2].concat([3,4]).join(\",\"));", "1,2,3,4\n");
}

#[test]
fn array_last_index_of() {
    assert_stdout(
        "let a = [1, 2, 1, 3]; console.log(a.lastIndexOf(1));",
        "2\n",
    );
}

#[test]
fn array_fill() {
    assert_stdout(
        "let a = [1, 2, 3]; console.log(a.fill(0).join(\",\"));",
        "0,0,0\n",
    );
}

#[test]
fn array_flat() {
    assert_stdout("console.log([[1,2],[3]].flat().join(\",\"));", "1,2,3\n");
}

#[test]
fn array_shift_unshift() {
    assert_stdout(
        "let a = [1, 2, 3]; console.log(a.shift()); console.log(a.join(\",\"));",
        "1\n2,3\n",
    );
    assert_stdout(
        "let a = [2, 3]; console.log(a.unshift(1)); console.log(a.join(\",\"));",
        "3\n1,2,3\n",
    );
}

#[test]
fn array_concat_in_template() {
    assert_stdout("console.log(\"x=\" + [1,2,3].join(\"-\"));", "x=1-2-3\n");
}

// ---- Bail tests: non-array receiver ----

#[test]
fn array_method_on_non_array_bails() {
    // `.indexOf` resolved as an Array method requires a proven-array receiver; on
    // a non-array (here a number variable) it is not an array receiver and the
    // number class has no `indexOf` → BAIL.
    assert_bails("let n = 5; console.log(n.indexOf(1));");
}
