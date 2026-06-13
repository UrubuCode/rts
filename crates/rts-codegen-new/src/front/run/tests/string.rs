//! P4: data-driven String instance-method dispatch via the Registry mirror —
//! `recv.method(args)` → the REAL `__RTS_FN_GL_STRING_*` symbol, no switchboard.

use super::assert_stdout;

#[test]
fn string_to_upper_case() {
    assert_stdout(r#"console.log("hello".toUpperCase());"#, "HELLO\n");
}

#[test]
fn string_to_lower_case() {
    assert_stdout(r#"console.log("HeLLo".toLowerCase());"#, "hello\n");
}

#[test]
fn string_trim() {
    assert_stdout(r#"console.log("  hi  ".trim());"#, "hi\n");
}

#[test]
fn string_index_of() {
    assert_stdout(r#"console.log("abcabc".indexOf("c"));"#, "2\n");
}

#[test]
fn string_repeat() {
    assert_stdout(r#"console.log("rts".repeat(3));"#, "rtsrtsrts\n");
}

#[test]
fn string_includes() {
    assert_stdout(r#"console.log("hello".includes("ell"));"#, "true\n");
    assert_stdout(r#"console.log("hello".includes("zzz"));"#, "false\n");
}

#[test]
fn string_char_at() {
    assert_stdout(r#"console.log("hello".charAt(1));"#, "e\n");
}

#[test]
fn string_slice() {
    assert_stdout(r#"console.log("hello".slice(1, 3));"#, "el\n");
}

#[test]
fn string_starts_ends_with() {
    assert_stdout(r#"console.log("hello".startsWith("he"));"#, "true\n");
    assert_stdout(r#"console.log("hello".endsWith("lo"));"#, "true\n");
}

#[test]
fn string_char_code_at() {
    assert_stdout(r#"console.log("A".charCodeAt(0));"#, "65\n");
}

#[test]
fn string_method_in_concat() {
    // The returned string PolyValue flows through the generic `+` path.
    assert_stdout(r#"console.log("a" + "b".toUpperCase());"#, "aB\n");
}
