import { describe, test, expect } from "rts:test";

// Arrow sem anotacao retornando string via metodo deve inferir string.
// Antes o handle saia como numero cru.
const up = (s: string) => s.toUpperCase();
const jn = (a: number[]) => a.join("-");
const ts = (n: number) => n.toString();
const cc = (s: string) => s.charAt(0);
const sl = (s: string) => s.slice(1);
const r1 = up("hi");
const r2 = jn([1, 2, 3]);
const r3 = ts(42) + "!";
const r4 = cc("xyz");
const r5 = sl("abc");

// Metodos numericos permanecem numero (sem regressao).
const idx = (a: number[]) => a.indexOf(2);
const len = (s: string) => s.length;
const n1 = idx([1, 2, 3]) + 100;
const n2 = len("abcd") + 1;

describe("arrow method string return", () => {
  test("toUpperCase", () => expect(r1).toBe("HI"));
  test("join", () => expect(r2).toBe("1-2-3"));
  test("toString", () => expect(r3).toBe("42!"));
  test("charAt", () => expect(r4).toBe("x"));
  test("slice", () => expect(r5).toBe("bc"));
  test("indexOf permanece numero", () => expect(n1).toBe(101));
  test("length permanece numero", () => expect(n2).toBe(5));
});
