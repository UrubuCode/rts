import { describe, test, expect } from "rts:test";

const it1 = Iterator.from([1, 2, 3]);
const a = it1.toArray().join(",");
const b = it1.toArray().join(",");

const it2 = Iterator.from([10, 20]);
const c = it2.toArray().join(",");
const inline = Iterator.from([7, 8, 9]).toArray().join("-");

describe("iterator helpers (#306)", () => {
  test("toArray consome do cursor", () => expect(a).toBe("1,2,3"));
  test("segunda chamada esgota", () => expect(b).toBe(""));
  test("inline Iterator.from().toArray()", () => expect(inline).toBe("7-8-9"));
  test("outro iterator independente", () => expect(c).toBe("10,20"));
});
