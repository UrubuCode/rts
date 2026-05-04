import { describe, test, expect } from "rts:test";

// Object.keys(arr) retorna ["0","1","2",...] (JS spec).

const arr = [10, 20, 30];
const k1 = Object.keys(arr);
const j1 = k1.join(",");
const len1 = k1.length;

const obj = { a: 1, b: 2, c: 3 };
const k2 = Object.keys(obj);
const j2 = k2.join(",");

describe("object_keys_array", () => {
  test("array keys = ['0','1','2']", () => expect(j1).toBe("0,1,2"));
  test("array keys.length = 3", () => expect(len1).toBe(3));
  test("object keys preserva ordem", () => expect(j2).toBe("a,b,c"));
});
