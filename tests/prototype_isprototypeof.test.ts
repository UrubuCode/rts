import { describe, test, expect } from "rts:test";

// (#247) `Object.prototype.isPrototypeOf(x)` (e variantes Array/etc)
// caia em "unsupported call expression form" porque o codegen
// renderiza `Object.prototype` como string sentinel handle, e o
// `.isPrototypeOf(...)` no sentinel nao tem builtin. Fix: detectar
// padrao `<Class>.prototype.isPrototypeOf(arg)` e tratar como
// `arg instanceof <Class>`.

const obj = {};
const arr: any[] = [];

const objProto = Object.prototype.isPrototypeOf(obj);
const arrProto = Array.prototype.isPrototypeOf(arr);
const objArr = Object.prototype.isPrototypeOf(arr);
const arrObj = Array.prototype.isPrototypeOf(obj);

describe("Object.prototype.isPrototypeOf (#247)", () => {
  test("Object.prototype.isPrototypeOf({})", () => expect(objProto).toBe(true));
  test("Array.prototype.isPrototypeOf([])", () => expect(arrProto).toBe(true));
  test("Object.prototype.isPrototypeOf([])", () => expect(objArr).toBe(true));
  test("Array.prototype.isPrototypeOf({})", () => expect(arrObj).toBe(false));
});
