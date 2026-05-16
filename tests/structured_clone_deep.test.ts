import { describe, test, expect } from "rts:test";

// (#97) structuredClone agora faz deep clone com cycle detection
// e preserva flags set_kind/map_kind. Antes era shallow e nao
// preservava self-references nem nested Set/Map flags.

const original: any = { count: 1, list: [1, 2, 3] };
original.self = original;

const copy: any = structuredClone(original);

const sameRef = copy.self === copy;
const cloned = copy !== original;
const countEq = copy.count === 1;

// Modificar copy nao afeta original (deep clone real)
copy.list.push(4);
const origListLen = original.list.length;
const copyListLen = copy.list.length;

const s = new Set([10, 20]);
const cs: any = structuredClone(s);
const cs_vals = Array.from(cs.values()).join(",");

describe("structuredClone deep + cycle (#97)", () => {
  test("self reference preservada no clone", () => expect(sameRef).toBe(true));
  test("copy diferente do original", () => expect(cloned).toBe(true));
  test("copy.count == 1", () => expect(countEq).toBe(true));
  test("mutacao do copy nao afeta original (deep)", () =>
    expect(origListLen).toBe(3));
  test("copy mutado tem 4 itens", () => expect(copyListLen).toBe(4));
  test("structuredClone(Set) preserva set_kind", () =>
    expect(cs_vals).toBe("10,20"));
});
