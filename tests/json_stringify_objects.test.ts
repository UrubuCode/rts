import { describe, test, expect } from "rts:test";

// (#562) JSON.stringify funciona em object/array literals (Map/Vec entries).

const a = JSON.stringify({ x: 1, y: 2 });
const b = JSON.stringify([1, 2, 3]);
const c = JSON.stringify("hello");
const d = JSON.stringify(42);
const e = JSON.stringify({ nested: [1, 2] });
const f = JSON.stringify({ str: "value" });

describe("json_stringify_objects", () => {
  test("object literal (#562)", () => expect(a).toBe('{"x":1,"y":2}'));
  test("array literal (#562)", () => expect(b).toBe("[1,2,3]"));
  test("string", () => expect(c).toBe('"hello"'));
  test("number", () => expect(d).toBe("42"));
  test("nested array (#562)", () => expect(e).toBe('{"nested":[1,2]}'));
  test("string value (#562)", () => expect(f).toBe('{"str":"value"}'));
});
