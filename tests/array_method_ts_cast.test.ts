import { describe, test, expect } from "rts:test";

// (#394) `arr.filter(fn) as T[]` — o cast TS envolvendo a call de array
// method impedia os passes de lift/rewrite de descer no TsAs, deixando o
// `arr.filter(inline_arrow)` cru chegar ao codegen -> SIGILL. Agora os
// passes descem em TsAs/TsTypeAssertion/TsNonNull.
const s = [1, 2, 3, 4];

// cast direto sobre filter
const evens = s.filter((v) => v % 2 === 0) as number[];
const evensStr = evens.join(",");

// cast sobre map
const doubled = s.map((v) => v * 2) as number[];
const doubledStr = doubled.join(",");

// destructuring + cast (caso do 394)
const [firstEven] = s.filter((v) => v > 2) as number[];

// cast aninhado em template/expr
const total = (s.filter((v) => v > 1) as number[]).length;

describe("array_method_ts_cast (#394)", () => {
  test("filter as T[]", () => expect(evensStr).toBe("2,4"));
  test("map as T[]", () => expect(doubledStr).toBe("2,4,6,8"));
  test("destructuring de filter cast", () => expect(firstEven).toBe(3));
  test("length de filter cast", () => expect(total).toBe(3));
});
