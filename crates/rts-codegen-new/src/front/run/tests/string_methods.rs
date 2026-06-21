//! String methods: Unicode normalize / isWellFormed / toWellFormed, dispatch on a
//! string VARIABLE (not only a literal), and indexOf / lastIndexOf with a fromIndex.

use super::assert_stdout;

#[test]
fn unicode_normalize_well_formed() {
    // NFD decomposes "é" into base + combining mark → 2 code units.
    assert_stdout("console.log(\"é\".normalize(\"NFD\").length);", "2\n");
    assert_stdout("console.log(\"abc\".isWellFormed());", "true\n");
    assert_stdout("console.log(\"abc\".toWellFormed());", "abc\n");
}

#[test]
fn methods_on_string_variable() {
    assert_stdout(
        r#"let s = "Hello World";
           console.log(s.toUpperCase());
           console.log(s.toLowerCase());
           console.log(s.length);"#,
        "HELLO WORLD\nhello world\n11\n",
    );
}

#[test]
fn string_var_method_chained_and_in_call() {
    assert_stdout(
        r#"let s = "  trim me  ";
           console.log(s.trim().toUpperCase());"#,
        "TRIM ME\n",
    );
    assert_stdout(
        r#"let s = "abc";
           console.log("[" + s.slice(1) + "]");"#,
        "[bc]\n",
    );
}

#[test]
fn index_of_from_index() {
    assert_stdout(r#"console.log("abcabc".indexOf("a", 1));"#, "3\n");
    assert_stdout(r#"console.log("abcabc".lastIndexOf("a", 2));"#, "0\n");
}
