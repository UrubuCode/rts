//! Chained access on a class FIELD holding an array (F1): `this.field[i]`
//! (read/write), `this.field.length`, `this.field.push(x)` — the foundation for
//! writing primitive wrapper classes (Array/Map/Set) in TypeScript.

use super::assert_stdout;

#[test]
fn field_array_push_length_index() {
    assert_stdout(
        r#"class Box {
              items: number[] = [];
              add(v: number): void { this.items.push(v); }
              at(i: number): number { return this.items[i]; }
              size(): number { return this.items.length; }
           }
           let b = new Box();
           b.add(10); b.add(20); b.add(30);
           console.log(b.size(), b.at(0), b.at(2));"#,
        "3 10 30\n",
    );
}

#[test]
fn field_array_index_write() {
    assert_stdout(
        r#"class Box {
              items: number[] = [];
              add(v: number): void { this.items.push(v); }
              put(i: number, v: number): void { this.items[i] = v; }
              at(i: number): number { return this.items[i]; }
           }
           let b = new Box();
           b.add(1); b.add(2); b.put(1, 99);
           console.log(b.at(0), b.at(1));"#,
        "1 99\n",
    );
}

#[test]
fn field_array_map_like() {
    assert_stdout(
        r#"class MyMap {
              keys: string[] = [];
              vals: number[] = [];
              set(k: string, v: number): void {
                for (let i = 0; i < this.keys.length; i++) {
                  if (this.keys[i] === k) { this.vals[i] = v; return; }
                }
                this.keys.push(k); this.vals.push(v);
              }
              get(k: string): number {
                for (let i = 0; i < this.keys.length; i++) {
                  if (this.keys[i] === k) return this.vals[i];
                }
                return -1;
              }
              has(k: string): boolean {
                for (let i = 0; i < this.keys.length; i++) {
                  if (this.keys[i] === k) return true;
                }
                return false;
              }
              size(): number { return this.keys.length; }
           }
           let m = new MyMap();
           m.set("a", 1); m.set("b", 2); m.set("a", 9);
           console.log(m.get("a"), m.get("b"), m.has("a"), m.has("z"), m.size());"#,
        "9 2 true false 2\n",
    );
}

// --- F2: private instance fields (`#name`) as ordinary field slots ---

#[test]
fn private_scalar_field() {
    assert_stdout(
        r#"class Counter {
              #n: number = 0;
              bump(): void { this.#n = this.#n + 1; }
              val(): number { return this.#n; }
           }
           let c = new Counter(); c.bump(); c.bump(); c.bump();
           console.log(c.val());"#,
        "3\n",
    );
}

#[test]
fn private_array_field_chained() {
    assert_stdout(
        r#"class Box {
              #items: number[] = [];
              add(v: number): void { this.#items.push(v); }
              at(i: number): number { return this.#items[i]; }
              size(): number { return this.#items.length; }
           }
           let b = new Box(); b.add(5); b.add(7);
           console.log(b.size(), b.at(0), b.at(1));"#,
        "2 5 7\n",
    );
}

#[test]
fn private_field_with_public_getter() {
    assert_stdout(
        r#"class Box {
              #items: number[] = [];
              push(v: number): void { this.#items.push(v); }
              get length(): number { return this.#items.length; }
           }
           let b = new Box(); b.push(1); b.push(2); b.push(3);
           console.log(b.length);"#,
        "3\n",
    );
}

#[test]
fn array_param_index_and_length() {
    assert_stdout(
        r#"function third(xs: number[]): number { return xs[2]; }
           function sumArr(xs: number[]): number {
             let t = 0;
             for (let i = 0; i < xs.length; i++) t = t + xs[i];
             return t;
           }
           console.log(third([10, 20, 30, 40]), sumArr([1, 2, 3, 4]));"#,
        "30 10\n",
    );
}

#[test]
fn array_param_on_method() {
    assert_stdout(
        r#"class Summer {
              total(xs: number[]): number {
                let t = 0;
                for (let i = 0; i < xs.length; i++) t = t + xs[i];
                return t;
              }
           }
           console.log(new Summer().total([5, 5, 5]));"#,
        "15\n",
    );
}

#[test]
fn let_from_array_returning_method() {
    assert_stdout(
        r#"let a = [1, 2, 3, 4];
           let s = a.slice(1, 3);
           console.log(s.length, s[0], s[1]);"#,
        "2 2 3\n",
    );
}
