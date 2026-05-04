import { describe, test, expect } from "rts:test";

// "abc".split("") = ["a","b","c"] (chars)
// Array.from("hello") = ["h","e","l","l","o"]

const a = "abc".split("");
const a_len = a.length;
const a_join = a.join("|");

const b = Array.from("hello");
const b_len = b.length;
const b_join = b.join(",");

const c = "x".split("");
const c_len = c.length;

const d = "abc".split(",");
const d_len = d.length;

describe("split_empty_array_from_string", () => {
  test("split('') returns chars", () => expect(a_len).toBe(3));
  test("split('') correct chars", () => expect(a_join).toBe("a|b|c"));
  test("Array.from(string) chars", () => expect(b_len).toBe(5));
  test("Array.from preserva ordem", () => expect(b_join).toBe("h,e,l,l,o"));
  test("split('') single char", () => expect(c_len).toBe(1));
  test("split sem match", () => expect(d_len).toBe(1));
});
