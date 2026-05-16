import { describe, test, expect } from "rts:test";

// (#749 follow-up) Object.getOwnPropertyDescriptors deve refletir
// writable/enumerable=false setados via Object.defineProperty. Antes
// eram sempre sintetizados como true; agora consulta non_writable_set/
// non_enumerable_set. Bools sao storage interno como sentinel; uso
// String() para comparar via stringification.

const obj: any = { a: 1 };
Object.defineProperty(obj, "b", { value: 2, writable: false, enumerable: false });
Object.defineProperty(obj, "c", { value: 3, writable: false });
Object.defineProperty(obj, "d", { value: 4, enumerable: false });

const descs: any = Object.getOwnPropertyDescriptors(obj);

const bStr = String(descs.b.writable) + "/" + String(descs.b.enumerable);
const cStr = String(descs.c.writable) + "/" + String(descs.c.enumerable);
const dStr = String(descs.d.writable) + "/" + String(descs.d.enumerable);
const aStr = String(descs.a.writable) + "/" + String(descs.a.enumerable);

describe("descriptor writable/enumerable reflete defineProperty (#749)", () => {
  test("b com writable+enumerable false", () => expect(bStr).toBe("false/false"));
  test("c so' writable false", () => expect(cStr).toBe("false/true"));
  test("d so' enumerable false", () => expect(dStr).toBe("true/false"));
  test("a sem flags -> default true", () => expect(aStr).toBe("true/true"));
});
