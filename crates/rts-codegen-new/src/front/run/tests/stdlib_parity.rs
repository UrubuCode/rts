//! B1 parity probe: a FAITHFUL `Map`/`Set` written in pure TypeScript, compiled
//! ahead of the user program via the explicit-prelude path
//! (`render_source_with_prelude`), must reproduce the exact native semantics the
//! `globalclass.rs` tests assert. This de-risks B3 (the later native deletion):
//! if the TS stdlib is a drop-in replacement here, registering it as an engine
//! include + deleting the native `__rtsadp_map_*`/`__rtsadp_set_*` is sound.
//!
//! NO native code is touched; this is purely a test-path probe.

use super::assert_stdout_with_prelude;

/// Faithful TS `Map`/`Set` — generic `<K,V>`, `=== ` key compare, `return this`
/// chaining, `delete` via shift+pop, `undefined` on miss, `get size()` getter.
/// This is the candidate `rts-primitives`/`rts-shared` `.ts` stdlib.
const STDLIB: &str = r#"
class Map<K, V> {
  #keys: K[] = [];
  #vals: V[] = [];
  set(k: K, v: V): Map<K, V> {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) { this.#vals[i] = v; return this; }
    }
    this.#keys.push(k); this.#vals.push(v);
    return this;
  }
  get(k: K): V | undefined {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) return this.#vals[i];
    }
    return undefined;
  }
  has(k: K): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) return true;
    }
    return false;
  }
  delete(k: K): boolean {
    for (let i = 0; i < this.#keys.length; i++) {
      if (this.#keys[i] === k) {
        for (let j = i; j < this.#keys.length - 1; j++) {
          this.#keys[j] = this.#keys[j + 1];
          this.#vals[j] = this.#vals[j + 1];
        }
        this.#keys.pop(); this.#vals.pop();
        return true;
      }
    }
    return false;
  }
  get size(): number { return this.#keys.length; }
}
class Set<T> {
  #items: T[] = [];
  add(v: T): Set<T> {
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) return this;
    }
    this.#items.push(v);
    return this;
  }
  has(v: T): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) return true;
    }
    return false;
  }
  delete(v: T): boolean {
    for (let i = 0; i < this.#items.length; i++) {
      if (this.#items[i] === v) {
        for (let j = i; j < this.#items.length - 1; j++) this.#items[j] = this.#items[j + 1];
        this.#items.pop();
        return true;
      }
    }
    return false;
  }
  get size(): number { return this.#items.length; }
}
"#;

// ---------------------------------------------------------------------------
// Map — exact expected strings mirror the native tests in `globalclass.rs`.
// ---------------------------------------------------------------------------

#[test]
fn map_set_get_size() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); m.set("a", 1); m.set("b", 2); console.log(m.get("a"), m.size);"#,
        "1 2\n",
    );
}

#[test]
fn map_has() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); m.set("a", 1); console.log(m.has("a"), m.has("z"));"#,
        "true false\n",
    );
}

#[test]
fn map_delete_then_size() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); m.set("a", 1); m.set("b", 2); m.delete("a"); console.log(m.size, m.has("a"));"#,
        "1 false\n",
    );
}

#[test]
fn map_string_values() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); m.set("name", "rts"); console.log(m.get("name"));"#,
        "rts\n",
    );
}

#[test]
fn map_get_missing_is_undefined() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); console.log(m.get("nope"));"#,
        "undefined\n",
    );
}

// ---------------------------------------------------------------------------
// Set.
// ---------------------------------------------------------------------------

#[test]
fn set_add_dedup_size_has() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let s = new Set(); s.add(1); s.add(1); s.add(2); console.log(s.size, s.has(1));"#,
        "2 true\n",
    );
}

#[test]
fn set_delete() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let s = new Set(); s.add(1); s.add(2); s.delete(1); console.log(s.size, s.has(1));"#,
        "1 false\n",
    );
}

#[test]
fn set_string_elements() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let s = new Set(); s.add("a"); s.add("a"); s.add("b"); console.log(s.size);"#,
        "2\n",
    );
}

// ---------------------------------------------------------------------------
// instanceof / typeof — the TS class must satisfy these identically.
// ---------------------------------------------------------------------------

#[test]
fn instanceof_map() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); console.log(m instanceof Map);"#,
        "true\n",
    );
}

#[test]
fn instanceof_set() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let s = new Set(); console.log(s instanceof Set);"#,
        "true\n",
    );
}

#[test]
fn instanceof_map_not_set() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); console.log(m instanceof Set);"#,
        "false\n",
    );
}

#[test]
fn typeof_map_object() {
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); console.log(typeof m);"#,
        "object\n",
    );
}

// ---------------------------------------------------------------------------
// EXTRA faithfulness checks the native tests don't cover but JS guarantees.
// ---------------------------------------------------------------------------

#[test]
fn map_numeric_vs_string_key_distinctness() {
    // JS guarantees `1 !== "1"`, so a numeric and a string key are distinct
    // entries. This passes (verified), confirming mixed number/string `===` is
    // correctly lowered (not mis-coerced) — the faithful Map keys soundly.
    assert_stdout_with_prelude(
        STDLIB,
        r#"let m = new Map(); m.set(1, "n"); m.set("1", "s"); console.log(m.get(1), m.get("1"), m.size);"#,
        "n s 2\n",
    );
}
