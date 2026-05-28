import { describe, test, expect } from "rts:test";

// Arrow sem anotacao retornando string via `||` / `??` (fallback string)
// deve inferir string. Antes o handle saia como numero cru.
const orFb = (s: string) => s || "default";
const nullFb = (s: string | null) => s ?? "none";
const r1 = orFb("");
const r2 = orFb("x");
const r3 = nullFb(null);

// Logical numerico permanece numero (sem regressao).
const orNum = (n: number) => n || 0;
const nullNum = (n: number) => n ?? -1;
const n1 = orNum(0) + 5;
const n2 = nullNum(7) + 1;

describe("arrow logical string return", () => {
  test("|| fallback string (vazio)", () => expect(r1).toBe("default"));
  test("|| valor truthy", () => expect(r2).toBe("x"));
  test("?? fallback string", () => expect(r3).toBe("none"));
  test("|| numerico permanece numero", () => expect(n1).toBe(5));
  test("?? numerico permanece numero", () => expect(n2).toBe(8));
});
