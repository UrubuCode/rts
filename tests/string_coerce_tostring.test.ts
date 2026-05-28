import { describe, test, expect } from "rts:test";

// String(obj) deve chamar o toString() custom da classe (#304 parcial).
// Antes devolvia sempre "[object Object]".
class Pt {
  x: number; y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
  toString(): string { return "(" + this.x + "," + this.y + ")"; }
}
const p = new Pt(3, 4);
const viaVar = String(p);
const viaNew = String(new Pt(1, 2));

// Classe sem toString -> [object Object] (comportamento JS).
class Bare { v: number = 1; }
const bare = String(new Bare());

// Primitivos nao afetados.
const num = String(42);
const bool = String(true);
const str = String("x");

describe("String() coerce toString", () => {
  test("String(var) chama toString", () => expect(viaVar).toBe("(3,4)"));
  test("String(new) chama toString", () => expect(viaNew).toBe("(1,2)"));
  test("sem toString vira object Object", () => expect(bare).toBe("[object Object]"));
  test("String(number)", () => expect(num).toBe("42"));
  test("String(boolean)", () => expect(bool).toBe("true"));
  test("String(string)", () => expect(str).toBe("x"));
});
