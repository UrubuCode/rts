import { describe, test, expect } from "rts:test";

// arr.toSpliced(start, count, ...items) com inserts (immutable).

const a1 = [1, 2, 3, 4];
const r1 = a1.toSpliced(1, 1, 99, 88);
const orig = a1.join(",");

const a2 = [1, 2, 3];
const r2 = a2.toSpliced(0, 0, 99); // insert no inicio.

describe("to_spliced_insert", () => {
  test("inserts no meio", () => expect(r1.join(",")).toBe("1,99,88,3,4"));
  test("array original imutavel", () => expect(orig).toBe("1,2,3,4"));
  test("insert no inicio", () => expect(r2.join(",")).toBe("99,1,2,3"));
});
