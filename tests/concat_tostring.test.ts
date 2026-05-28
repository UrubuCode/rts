import { describe, test, expect } from "rts:test";

// `"x" + obj` / `obj + "x"` deve chamar toString() custom (#304 cont.).
// Antes devolvia "[object Object]".
class Pt {
  x: number; y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
  toString(): string { return "(" + this.x + "," + this.y + ")"; }
}
const p = new Pt(3, 4);
const prefix = "p=" + p;
const suffix = p + " end";
const inlineNew = "new: " + new Pt(5, 6);

// Operator overload nao regride: a.add(b).v continua numero.
class V {
  v: number;
  constructor(v: number) { this.v = v; }
  add(o: V): V { return new V(this.v + o.v); }
  toString(): string { return "V" + this.v; }
}
const sum = new V(2).add(new V(3)).v;

// Classe sem toString -> [object Object].
class Bare { n: number = 1; }
const bare = "bare=" + new Bare();

describe("concat toString (#304)", () => {
  test("prefixo string + obj", () => expect(prefix).toBe("p=(3,4)"));
  test("obj + sufixo string", () => expect(suffix).toBe("(3,4) end"));
  test("string + new C()", () => expect(inlineNew).toBe("new: (5,6)"));
  test("operator overload intacto", () => expect(sum).toBe(5));
  test("sem toString vira object Object", () => expect(bare).toBe("bare=[object Object]"));
});
