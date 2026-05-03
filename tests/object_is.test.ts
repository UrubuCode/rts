import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Object.is — egal-style equality.

// Primitivos
out += (Object.is(1, 1) ? "y" : "n") + "\n";
out += (Object.is(NaN, NaN) ? "y" : "n") + "\n";   // diferente de ===
out += (Object.is(0, 1) ? "y" : "n") + "\n";
out += (Object.is("a", "a") ? "y" : "n") + "\n";
out += (Object.is(true, true) ? "y" : "n") + "\n";

// Object literais — comparacao por identidade
const a = { x: 1 };
const b = { x: 1 };
const c = a;
out += (Object.is(a, a) ? "y" : "n") + "\n";  // mesma ref
out += (Object.is(a, b) ? "y" : "n") + "\n";  // refs diferentes
out += (Object.is(a, c) ? "y" : "n") + "\n";  // alias

// Classes derivadas — identidade preservada via upcast
class Animal { name: string; constructor(n: string) { this.name = n; } }
class Dog extends Animal { constructor(n: string) { super(n); } }
const d1 = new Dog("Rex");
const d2 = new Dog("Rex");
const d3 = d1;
const a1: Animal = d1;
out += (Object.is(d1, d2) ? "y" : "n") + "\n";
out += (Object.is(d1, d3) ? "y" : "n") + "\n";
out += (Object.is(d1, a1) ? "y" : "n") + "\n";  // upcast preserva ref

describe("object_is", () => {
  test("egal equality cobre primitivos, objetos e classes derivadas", () =>
    expect(out).toBe("y\ny\nn\ny\ny\ny\nn\ny\nn\ny\ny\n"));
});
