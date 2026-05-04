import { describe, test, expect } from "rts:test";

// JSON.parse retorna Map/Vec navegavel via .x e [i].

const obj = JSON.parse('{"x": 1, "y": 2}');
const x = obj.x;
const y = obj.y;

const arr: number[] = JSON.parse('[10, 20, 30]');
const a0 = arr[0];
const a2 = arr[2];
const len = arr.length;

const nested = JSON.parse('{"a": {"b": 42}}');
const nb = nested.a.b;

describe("json_parse_access", () => {
  test("obj.x", () => expect(x).toBe(1));
  test("obj.y", () => expect(y).toBe(2));
  test("arr[0]", () => expect(a0).toBe(10));
  test("arr[2]", () => expect(a2).toBe(30));
  test("arr.length", () => expect(len).toBe(3));
  test("nested.a.b", () => expect(nb).toBe(42));
});
