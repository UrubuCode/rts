//! Prelude mechanism (B1): a stdlib CLASS written in TypeScript, compiled ahead
//! of the user program, shadows the native class end-to-end. This proves the
//! "primitives/utilities in TS" direction: `class Map` in TS replaces native Map.

use super::assert_stdout_with_prelude;

/// A minimal `Map` written in pure TypeScript (string keys, number values),
/// backed by two private arrays — the kind of file that will live in
/// `rts-primitives`/`rts-shared` as `.ts` and replace the Rust `__rtsadp_map_*`.
/// (`delete` is omitted: it needs `Array.splice`, which the new engine does not
/// yet support; the set/get/has/size core is the proof.)
const MAP_TS: &str = r#"
class Map {
  #keys: string[] = [];
  #vals: number[] = [];
  set(k: string, v: number): void {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) { this.#vals[i] = v; return; }
    }
    this.#keys.push(k); this.#vals.push(v);
  }
  get(k: string): number {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) return this.#vals[i];
    }
    return -1;
  }
  has(k: string): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) return true;
    }
    return false;
  }
  get size(): number { return this.#keys.length; }
}
"#;

#[test]
fn ts_map_shadows_native() {
    assert_stdout_with_prelude(
        MAP_TS,
        r#"let m = new Map();
           m.set("a", 1); m.set("b", 2); m.set("a", 9);
           console.log(m.get("a"), m.get("b"), m.has("a"), m.has("z"), m.size);"#,
        "9 2 true false 2\n",
    );
}

#[test]
fn ts_map_instanceof_and_typeof() {
    assert_stdout_with_prelude(
        MAP_TS,
        r#"let m = new Map();
           console.log(m instanceof Map, typeof m);"#,
        "true object\n",
    );
}

#[test]
fn prelude_function_ambient() {
    assert_stdout_with_prelude(
        r#"function triple(x: number): number { return x * 3; }"#,
        r#"console.log(triple(7));"#,
        "21\n",
    );
}
