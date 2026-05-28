import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `for (const ch of h.text)` / `this.content` onde
// o field eh `: string` nao iterava. for_of_iterates_string nao cobria
// Expr::Member. Fix: resolve a classe do receiver (this/ident) e checa
// field_class_names == "string" na hierarquia. Fecha a familia for-of-string
// (#1270 param, #1271 fn/concat).

let out = "";
function print(v: string): void { out += v + "\n"; }

class Holder { text: string = "hi"; }
const h = new Holder();
let r1 = "";
for (const ch of h.text) r1 += ch + ".";
print(r1);                       // h.i.

class Doc {
  content: string = "abc";
  chars(): string {
    let s = "";
    for (const ch of this.content) s += ch + "-";
    return s;
  }
}
print(new Doc().chars());        // a-b-c-

// field herdado
class Base { label: string = "xy"; }
class Derived extends Base {}
const d = new Derived();
let r3 = "";
for (const ch of d.label) r3 += ch;
print(r3);                       // xy

describe("for-of field string", () => {
  test("for-of sobre field/this string itera chars", () =>
    expect(out).toBe("h.i.\na-b-c-\nxy\n"));
});
