import { describe, test, expect } from "rts:test";

// arr.splice(start, deleteCount, ...items) — fase 2 com inserts.

const a1 = [1, 2, 3, 4, 5];
const r1 = a1.splice(1, 2, 10, 20);

const a2 = [1, 2, 3];
const r2 = a2.splice(1, 0, 99); // insert sem remover.

const a3 = [1, 2, 3, 4];
const r3 = a3.splice(2, 1, 88, 77); // remove 1 + insert 2.

describe("array_splice_insert", () => {
  test("array apos splice com insert", () => expect(a1.join(",")).toBe("1,10,20,4,5"));
  test("removed values", () => expect(r1.join(",")).toBe("2,3"));
  test("insert sem remover", () => expect(a2.join(",")).toBe("1,99,2,3"));
  test("insert sem remover - removed vazio", () => expect(r2.length).toBe(0));
  test("net add (remove 1, insert 2)", () => expect(a3.join(",")).toBe("1,2,88,77,4"));
  test("net add - removed", () => expect(r3.join(",")).toBe("3"));
});
