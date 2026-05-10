// Cobertura do fix de logical || / && com string vazia.
// Antes: `'' || 'fb'` retornava `''` porque codegen comparava handle != 0
// (handle de string vazia eh != 0). Fix: detecta Handle e usa
// __RTS_FN_RT_TRUTHY que reconhece string vazia como falsy (JS spec).
import { describe, test, expect } from "rts:test";

const empty: string = "";
const filled: string = "value";

const orEmpty: string = empty || "fb";
const orFilled: string = filled || "fb";
const orLit1: string = "" || "fb";
const orLit2: string = "x" || "fb";
const andEmpty: string = empty && "x";
const andFilled: string = filled && "x";

// Combo com number tambem
const numZero: number = 0;
const numNonZero: number = 5;
const orNumZero: number = numZero || -1;
const orNumNonZero: number = numNonZero || -1;

// Real-world: default value pattern
function getName(name: string): string {
  return name || "Anonymous";
}

describe("logical || && com string vazia (#or-empty-str)", () => {
  test("'' || 'fb' = 'fb'", () => expect(orEmpty).toBe("fb"));
  test("'value' || 'fb' = 'value'", () => expect(orFilled).toBe("value"));
  test("'' || 'fb' literal = 'fb'", () => expect(orLit1).toBe("fb"));
  test("'x' || 'fb' literal = 'x'", () => expect(orLit2).toBe("x"));
  test("'' && 'x' = ''", () => expect(andEmpty).toBe(""));
  test("'value' && 'x' = 'x'", () => expect(andFilled).toBe("x"));
  // number paths (regressao guard)
  test("0 || -1 = -1", () => expect(orNumZero).toBe(-1));
  test("5 || -1 = 5", () => expect(orNumNonZero).toBe(5));
  // Pattern comum
  test("getName('') = 'Anonymous'", () => expect(getName("")).toBe("Anonymous"));
  test("getName('Bob') = 'Bob'", () => expect(getName("Bob")).toBe("Bob"));
});
