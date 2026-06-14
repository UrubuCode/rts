//! P5.12: RegExp literals + `new RegExp(..)`, `.test`, and the string-with-regex
//! methods `.match`/`.replace`/`.replaceAll`/`.split`/`.search`. Each program runs
//! end to end through the REAL `__RTS_FN_NS_REGEX_*` runtime symbols (the `regex`
//! crate / RE2), asserting exact captured stdout against what Bun/Node prints.
//!
//! Bails are the honesty floor: a function replacer, capture-group `.exec`.

use super::{assert_bails, assert_stdout};

#[test]
fn regex_literal_test() {
    // `/\d+/.test(s)` — a regex literal as a bare receiver.
    assert_stdout(
        r#"console.log(/\d+/.test("abc123"), /\d+/.test("xyz"));"#,
        "true false\n",
    );
}

#[test]
fn regex_new_test() {
    assert_stdout(
        r#"let re = new RegExp("a.c"); console.log(re.test("axc"));"#,
        "true\n",
    );
}

#[test]
fn regex_new_with_flags_test() {
    assert_stdout(
        r#"let re = new RegExp("abc", "i"); console.log(re.test("ABC"));"#,
        "true\n",
    );
}

#[test]
fn regex_literal_local_test() {
    // `let re = /pat/` records the RegExp class for later `re.test(..)`.
    assert_stdout(
        r#"let re = /\d/; console.log(re.test("a1"), re.test("ab"));"#,
        "true false\n",
    );
}

#[test]
fn string_replace_regex_first() {
    // First match only (no `g` flag).
    assert_stdout(r##"console.log("a1b2c3".replace(/\d/, "#"));"##, "a#b2c3\n");
}

#[test]
fn string_replace_regex_global() {
    // The `g` flag routes `.replace` to the replace-all trampoline.
    assert_stdout(r##"console.log("a1b2c3".replace(/\d/g, "#"));"##, "a#b#c#\n");
}

#[test]
fn string_replace_all_regex() {
    assert_stdout(r##"console.log("a1b2c3".replaceAll(/\d/, "#"));"##, "a#b#c#\n");
}

#[test]
fn string_replace_regex_ignorecase() {
    assert_stdout(r#"console.log("AbC".replace(/b/i, "_"));"#, "A_C\n");
}

#[test]
fn string_split_regex() {
    assert_stdout(
        r#"console.log("a, b ,c".split(/\s*,\s*/).join("|"));"#,
        "a|b|c\n",
    );
}

#[test]
fn string_search_regex() {
    assert_stdout(r#"console.log("hello world".search(/world/));"#, "6\n");
}

#[test]
fn string_search_regex_no_match() {
    assert_stdout(r#"console.log("hello".search(/zzz/));"#, "-1\n");
}

#[test]
fn string_match_regex() {
    // `.match` returns an array; `[0]` is the first match.
    assert_stdout(r#"let m = "abc123".match(/\d+/); console.log(m[0]);"#, "123\n");
}

#[test]
fn string_match_regex_global_count() {
    // A global regex returns all matches; the array length is the count.
    assert_stdout(
        r#"let m = "a1b2c3".match(/\d/g); console.log(m.length);"#,
        "3\n",
    );
}

#[test]
fn regex_source_property() {
    assert_stdout(r#"let re = /a.c/; console.log(re.source);"#, "a.c\n");
}

#[test]
fn regex_flags_property() {
    assert_stdout(r#"let re = /x/gi; console.log(re.flags);"#, "gi\n");
}

#[test]
fn regex_global_property() {
    assert_stdout(
        r#"let a = /x/g; let b = /y/; console.log(a.global, b.global);"#,
        "true false\n",
    );
}

#[test]
fn regex_ignorecase_multiline_properties() {
    assert_stdout(
        r#"let re = /x/im; console.log(re.ignoreCase, re.multiline);"#,
        "true true\n",
    );
}

#[test]
fn regex_lastindex_initial() {
    // A fresh regex has lastIndex 0.
    assert_stdout(r#"let re = /x/g; console.log(re.lastIndex);"#, "0\n");
}

#[test]
fn regex_typeof_object() {
    assert_stdout(r#"let re = /x/; console.log(typeof re);"#, "object\n");
}

// ── Bails (the honesty floor) ───────────────────────────────────────────────

#[test]
fn regex_function_replacer_bails() {
    // A function replacer is a later increment — must bail, not produce a wrong
    // value.
    assert_bails(r#"console.log("a1".replace(/\d/, (m) => m + m));"#);
}
