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
