// Cobertura do fix do bridge JSON: `JSON.parse(...).length` antes
// resolvia via MAP_GET("length") -> 0 -> "null" no template. Fix
// adicionou helper UNIVERSAL_LENGTH e plugou em lower_member quando
// receiver eh U64 (handle opaco de extern call).
import { describe, test, expect } from "rts:test";

const arr: any = JSON.parse("[10,20,30]");
const obj: any = JSON.parse('{"name":"alice","age":30}');
const str: any = JSON.parse('"hello"');

const stArr: string = JSON.stringify([1, 2, 3]);
const stObj: string = JSON.stringify({ a: 1, b: "hi" });
const stEmpty: string = JSON.stringify({});

describe("JSON.parse + .length / .field (#json-bridge)", () => {
  test("parse array .length", () => expect(arr.length).toBe(3));
  test("parse array [0]", () => expect(arr[0]).toBe(10));
  test("parse array [1]", () => expect(arr[1]).toBe(20));
  test("parse array [2]", () => expect(arr[2]).toBe(30));
  test("parse object .name", () => expect(obj.name).toBe("alice"));
  test("parse object .age", () => expect(obj.age).toBe(30));
  test("parse string .length", () => expect(str.length).toBe(5));
  test("stringify array", () => expect(stArr).toBe("[1,2,3]"));
  test("stringify object", () => expect(stObj).toBe('{"a":1,"b":"hi"}'));
  test("stringify empty obj", () => expect(stEmpty).toBe("{}"));
});
