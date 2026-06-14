//! Native-string class wrap pattern: a class field typed `string` is a native
//! string and supports the engine's existing native string ops wherever it
//! appears — a `#value: string` field's `.length`, indexing `this.#value[i]`
//! (→ 1-char string), and string methods (`charCodeAt`), plus the same ops on a
//! string param (`sub`) and a string literal receiver (`"abc".length` / `"abc"[1]`).
//!
//! This is the foundation for writing primitive wrapper classes (the canonical
//! `class String { #value: string; get length() { return this.#value.length } }`)
//! in TypeScript over the native string primitive.

use super::assert_stdout_with_prelude;

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
