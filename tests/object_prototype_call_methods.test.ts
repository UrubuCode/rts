import { describe, test, expect } from "rts:test";

// (#40) `Object.prototype.hasOwnProperty.call(obj, key)` (e .propertyIsEnumerable)
// falhava em "unsupported call expression form" porque
// `Object.prototype.hasOwnProperty` virava string sentinel handle e
// `.call(...)` no sentinel nao tinha builtin.
// Fix: reescreve como `obj.hasOwnProperty(key)` no codegen.

const obj: any = { a: 1, b: "x" };

const ownA = Object.prototype.hasOwnProperty.call(obj, "a");
const ownZ = Object.prototype.hasOwnProperty.call(obj, "z");
const enumA = Object.prototype.propertyIsEnumerable.call(obj, "a");
const enumZ = Object.prototype.propertyIsEnumerable.call(obj, "z");

describe("Object.prototype.<method>.call(obj, ...) (#40)", () => {
  test("hasOwnProperty.call obj 'a' eh true", () => expect(ownA).toBe(true));
  test("hasOwnProperty.call obj 'z' eh false", () => expect(ownZ).toBe(false));
  test("propertyIsEnumerable.call obj 'a' eh true", () => expect(enumA).toBe(true));
  test("propertyIsEnumerable.call obj 'z' eh false", () => expect(enumZ).toBe(false));
});
