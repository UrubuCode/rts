import { describe, test, expect } from "rts:test";

// JSON.stringify(value, replacer, indent) com indent.

const r1 = JSON.stringify({ a: 1, b: 2 }, null, 2);
const r2 = JSON.stringify([1, 2, 3], null, 2);
const r3 = JSON.stringify({ nested: { x: 1 } }, null, 2);

describe("json_stringify_pretty", () => {
  test("object simples", () => expect(r1).toBe('{\n  "a": 1,\n  "b": 2\n}'));
  test("array", () => expect(r2).toBe("[\n  1,\n  2,\n  3\n]"));
  test("nested", () => expect(r3).toBe('{\n  "nested": {\n    "x": 1\n  }\n}'));
});
