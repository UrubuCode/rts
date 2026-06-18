//! Native-string class wrap pattern: a class field typed `string` is a native
//! string and supports the engine's existing native string ops wherever it
//! appears — a `#value: string` field's `.length`, indexing `this.#value[i]`
//! (→ 1-char string), and string methods (`charCodeAt`), plus the same ops on a
//! string param (`sub`) and a string literal receiver (`"abc".length` / `"abc"[1]`).
//!
//! This is the foundation for writing primitive wrapper classes (the canonical
//! `class String { #value: string; get length() { return this.#value.length } }`)
//! in TypeScript over the native string primitive.

use super::{assert_stdout, assert_stdout_with_prelude};

/// The canonical native-string wrap class + a couple of pure-TS methods written
/// over native string ops (length / indexing / charCodeAt).
const PRELUDE: &str = r#"
class MyStr {
  #value: string;
  constructor(v: string) { this.#value = v; }
  get length(): number { return this.#value.length; }
  at(i: number): string { return this.#value[i]; }
  code(i: number): number { return this.#value.charCodeAt(i); }
  upper(): string {
    let r = ''; let i = 0;
    while (i < this.#value.length) {
      const c = this.#value.charCodeAt(i);
      if (c >= 97 && c <= 122) { r += String.fromCharCode(c - 32); } else { r += this.#value[i]; }
      i++;
    }
    return r;
  }
  indexOf(sub: string): number {
    let i = 0;
    while (i <= this.#value.length - sub.length) {
      let ok = true; let j = 0;
      while (j < sub.length) { if (this.#value[i + j] !== sub[j]) { ok = false; break; } j++; }
      if (ok) return i; i++;
    }
    return -1;
  }
}
"#;

#[test]
fn string_field_length_index_charcode() {
    assert_stdout_with_prelude(
        PRELUDE,
        r#"let s = new MyStr("Hello"); console.log(s.length, s.at(1), s.code(1));"#,
        "5 e 101\n",
    );
}

#[test]
fn string_field_upper_over_native_ops() {
    assert_stdout_with_prelude(
        PRELUDE,
        r#"let s = new MyStr("hello"); console.log(s.upper());"#,
        "HELLO\n",
    );
}

#[test]
fn string_param_index_and_length() {
    assert_stdout_with_prelude(
        PRELUDE,
        r#"let s = new MyStr("Hello World"); console.log(s.indexOf("World"));"#,
        "6\n",
    );
}

#[test]
fn string_literal_receiver_length_and_index() {
    assert_stdout_with_prelude(PRELUDE, r#"console.log("abc".length, "abc"[1]);"#, "3 b\n");
}

// ===========================================================================
// PRIMORDIAL `String.prototype` methods migrated to the prelude `.ts`
// `class String` (`rts-primitives/src/string.ts`). A method called on a
// PRIMITIVE string receiver is routed into the ambient class with the primitive
// BOXED as `this` (the boolean/number primitive→prelude mechanism). The `.ts`
// bodies call the PRIVATE `engine.str_*` helpers (the irreducible Unicode-aware
// Rust impls — one source of truth). Each test runs a REAL program end to end
// and asserts EXACT captured stdout (the honesty floor).
// ===========================================================================

#[test]
fn upper_case() {
    assert_stdout(r#"console.log("abc".toUpperCase());"#, "ABC\n");
}

#[test]
fn lower_case() {
    assert_stdout(r#"console.log("AbC".toLowerCase());"#, "abc\n");
}

#[test]
fn trim() {
    assert_stdout(r#"console.log("[" + "  x ".trim() + "]");"#, "[x]\n");
}

#[test]
fn trim_start_end() {
    assert_stdout(
        r#"console.log("[" + "  x  ".trimStart() + "]", "[" + "  x  ".trimEnd() + "]");"#,
        "[x  ] [  x]\n",
    );
}

#[test]
fn slice_two_args() {
    assert_stdout(r#"console.log("abcdef".slice(1, 3));"#, "bc\n");
}

#[test]
fn slice_one_arg_to_end() {
    // The `.ts` default-param `end = 2147483647` clamps to length ("to end").
    assert_stdout(r#"console.log("abcdef".slice(2));"#, "cdef\n");
}

#[test]
fn substring_two_args() {
    assert_stdout(r#"console.log("abcdef".substring(1, 4));"#, "bcd\n");
}

#[test]
fn char_at() {
    assert_stdout(r#"console.log("abc".charAt(1));"#, "b\n");
}

#[test]
fn char_code_at() {
    assert_stdout(r#"console.log("abc".charCodeAt(0));"#, "97\n");
}

#[test]
fn at_negative() {
    assert_stdout(r#"console.log("abc".at(-1));"#, "c\n");
}

#[test]
fn index_of() {
    assert_stdout(r#"console.log("abcabc".indexOf("c"));"#, "2\n");
}

#[test]
fn last_index_of() {
    assert_stdout(r#"console.log("abcabc".lastIndexOf("a"));"#, "3\n");
}

#[test]
fn includes() {
    assert_stdout(r#"console.log("abc".includes("b"));"#, "true\n");
}

#[test]
fn starts_with() {
    assert_stdout(r#"console.log("abc".startsWith("ab"));"#, "true\n");
}

#[test]
fn ends_with() {
    assert_stdout(r#"console.log("abc".endsWith("bc"));"#, "true\n");
}

#[test]
fn repeat() {
    assert_stdout(r#"console.log("ab".repeat(3));"#, "ababab\n");
}

#[test]
fn pad_start() {
    assert_stdout(r#"console.log("5".padStart(3, "0"));"#, "005\n");
}

#[test]
fn pad_start_default_space() {
    // The `.ts` default-param `pad = " "` (JS spec).
    assert_stdout(r#"console.log("[" + "5".padStart(3) + "]");"#, "[  5]\n");
}

#[test]
fn pad_end() {
    assert_stdout(r#"console.log("5".padEnd(3, "0"));"#, "500\n");
}

#[test]
fn concat() {
    assert_stdout(r#"console.log("a".concat("b", "c"));"#, "abc\n");
}

#[test]
fn replace_string() {
    assert_stdout(r#"console.log("a-b".replace("-", "_"));"#, "a_b\n");
}

#[test]
fn replace_all_string() {
    assert_stdout(r#"console.log("a-b-c".replaceAll("-", "_"));"#, "a_b_c\n");
}

#[test]
fn method_chaining() {
    assert_stdout(r#"console.log("  AbC ".trim().toLowerCase());"#, "abc\n");
}

#[test]
fn method_on_string_variable() {
    assert_stdout(r#"let s = "hello"; console.log(s.toUpperCase());"#, "HELLO\n");
}

#[test]
fn method_on_computed_string() {
    // The receiver is a proven-string `+` result, routed like a literal.
    assert_stdout(r#"console.log(("a" + "b").toUpperCase());"#, "AB\n");
}

// ---------------------------------------------------------------------------
// KEPT on the engine's own paths (NOT migrated to `.ts`): `split` (returns an
// ARRAY, stays on `try_string_special`) + the deprecated `substr`. Regression
// guards that they still work after the migration narrowed `STRING_ROWS`.
// ---------------------------------------------------------------------------

#[test]
fn split_still_works() {
    // `split` stays on `try_string_special` (returns an ARRAY, not migrated to the
    // `.ts` class). Bind the result first (indexing a fresh call-result array is a
    // separate, pre-existing limitation — see `string::string_split_*`).
    assert_stdout(
        r#"let p = "a,b,c".split(","); console.log(p[1], p.length);"#,
        "b 3\n",
    );
}

#[test]
fn substr_still_works() {
    assert_stdout(r#"console.log("abcdef".substr(1, 3));"#, "bcd\n");
}

// ---------------------------------------------------------------------------
// `String(x)` FACTORY (no `new`) — the `.ts` `StringFactory` returns a PRIMITIVE
// string (typeof === "string") via the engine's own ToString.
// ---------------------------------------------------------------------------

#[test]
fn factory_returns_primitive() {
    assert_stdout("console.log(typeof String(42));", "string\n");
}

#[test]
fn factory_number() {
    assert_stdout("console.log(String(42));", "42\n");
}

#[test]
fn factory_bool() {
    assert_stdout("console.log(String(true));", "true\n");
}

#[test]
fn factory_array_joins() {
    assert_stdout("console.log(String([1, 2, 3]));", "1,2,3\n");
}

// ---------------------------------------------------------------------------
// `new String(x)` WRAPPER — now the `.ts` `class String` constructed via the
// normal user-class path (a shape-based object, typeof === "object"). Its methods
// + `.length` getter route to the SAME `.ts` bodies as the primitive autobox path
// (dual `this`), reading the primitive from the `__prim` slot.
// ---------------------------------------------------------------------------

#[test]
fn new_string_is_object() {
    assert_stdout(r#"console.log(typeof new String("x"));"#, "object\n");
}

#[test]
fn new_string_instanceof() {
    assert_stdout(r#"console.log(new String("x") instanceof String);"#, "true\n");
}

#[test]
fn wrapper_to_string() {
    assert_stdout(r#"console.log(new String("Hello").toString());"#, "Hello\n");
}

#[test]
fn wrapper_to_upper() {
    assert_stdout(r#"console.log(new String("Hello").toUpperCase());"#, "HELLO\n");
}

#[test]
fn wrapper_length() {
    assert_stdout(r#"console.log(new String("Hello").length);"#, "5\n");
}

#[test]
fn wrapper_char_at() {
    assert_stdout(r#"console.log(new String("Hello").charAt(1));"#, "e\n");
}

#[test]
fn wrapper_coerces_number_arg() {
    assert_stdout(r#"console.log(new String(42).valueOf());"#, "42\n");
}
