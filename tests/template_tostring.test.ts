import { describe, test, expect } from "rts:test";

// `${obj}` em template literal deve chamar toString() custom (#304 cont.).
// Antes devolvia "[object Object]".
class Pt {
  x: number; y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
  toString(): string { return "(" + this.x + "," + this.y + ")"; }
}
const p = new Pt(3, 4);
const mid = `point ${p} done`;
const only = `${p}`;
const inlineNew = `${new Pt(5, 6)}`;

// Template normal nao regride.
const n = 42; const s = "hi";
const normal = `n=${n} s=${s} sum=${n + 1}`;

// Classe sem toString -> [object Object].
class Bare { v: number = 1; }
const bare = `bare=${new Bare()}`;

describe("template toString (#304)", () => {
  test("obj no meio do template", () => expect(mid).toBe("point (3,4) done"));
  test("template so com obj", () => expect(only).toBe("(3,4)"));
  test("new C() no template", () => expect(inlineNew).toBe("(5,6)"));
  test("template normal intacto", () => expect(normal).toBe("n=42 s=hi sum=43"));
  test("sem toString vira object Object", () => expect(bare).toBe("bare=[object Object]"));
});
