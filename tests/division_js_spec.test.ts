// (#584) `/` em JS sempre retorna f64. Cobertura explicita pra que
// futuros refactors nao reintroduzam int division.

import { describe, test, expect } from "rts:test";

const a = 1 / 3;       // 0.333...
const b = 7 / 2;       // 3.5
const c = 10 / 4;      // 2.5
const d = 6 / 2;       // 3.0 (formatado "3")
const e = -8 / 4;      // -2.0
const f = 5 / 0;       // Infinity (IEEE)
const g = -5 / 0;      // -Infinity
const h = 0 / 0;       // NaN

// Combinado com ops que esperam f64 — confirma que o resultado e' f64.
const i = (10 / 4) + 0.5;   // 3.0
const j = (1 / 2) * 4;      // 2.0

describe("division_js_spec", () => {
    test("basic fraction", () => expect(a).toBe(0.3333333333333333));
    test("half-integer", () => expect(b).toBe(3.5));
    test("quarter", () => expect(c).toBe(2.5));
    test("integer-result-is-still-f64", () => expect(d).toBe(3));
    test("negative", () => expect(e).toBe(-2));
    test("infinity", () => expect(f).toBe(Infinity));
    test("negative infinity", () => expect(g).toBe(-Infinity));
    test("nan", () => expect(h).toBe(NaN));
    test("compose with add", () => expect(i).toBe(3));
    test("compose with mul", () => expect(j).toBe(2));
});
