import { describe, test, expect } from "rts:test";

// (#549, #553) Array literal/expr como receiver de method-call
// agora dispatcha para array_builtin quando obj nao e Ident.

const r1 = [1, 2, 3].reverse().join(",");
const r2 = [1, 2, 3, 4, 5].slice(1, 4).join(",");
const r3 = [1, 2].concat([3, 4]).join(",");
const r4 = [0, 0, 0].fill(7).join(",");
const r5 = [[1, 2], [3, 4]].flat().join(",");
const r6 = [3, 1, 2].toSorted().join(",");
const r7 = [1, 2, 3].toReversed().join(",");
const r8 = "a,b,c,d".split(",", 2).join("|");

describe("array_literal_method_chain", () => {
  test("reverse (#549)", () => expect(r1).toBe("3,2,1"));
  test("slice (#549)", () => expect(r2).toBe("2,3,4"));
  test("concat (#549)", () => expect(r3).toBe("1,2,3,4"));
  test("fill (#549)", () => expect(r4).toBe("7,7,7"));
  test("flat (#549)", () => expect(r5).toBe("1,2,3,4"));
  test("toSorted (#549)", () => expect(r6).toBe("1,2,3"));
  test("toReversed (#549)", () => expect(r7).toBe("3,2,1"));
  test("split limit (#553)", () => expect(r8).toBe("a|b"));
});
