import { describe, test, expect } from "rts:test";

// Arrow `const f = (n) => ...ternary...` sem anotacao de retorno, cujos
// ramos sao todos string, deve inferir retorno string. Antes o ternary
// (especialmente encadeado) nao era detectado e o handle saia como numero.
const simple = (n: number) => n >= 90 ? "A" : "B";
const chained = (n: number) => n >= 90 ? "A" : n >= 80 ? "B" : "C";
const s1 = simple(95);
const s2 = simple(50);
const c1 = chained(95);
const c2 = chained(85);
const c3 = chained(70);

// Arrow ternary numerico permanece numero (sem regressao).
const num = (n: number) => n >= 0 ? 1 : -1;
const n1 = num(5);
const n2 = num(-5);

describe("arrow ternary string return", () => {
  test("ternary simples ramo A", () => expect(s1).toBe("A"));
  test("ternary simples ramo B", () => expect(s2).toBe("B"));
  test("ternary encadeado A", () => expect(c1).toBe("A"));
  test("ternary encadeado B", () => expect(c2).toBe("B"));
  test("ternary encadeado C", () => expect(c3).toBe("C"));
  test("ternary numerico positivo", () => expect(n1).toBe(1));
  test("ternary numerico negativo", () => expect(n2).toBe(-1));
});
