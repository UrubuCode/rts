// Cobertura do fix de String(null)/String(undefined) e null==undefined.
// Validado contra Bun 1.x e Node 22 — saidas iguais.
import { describe, test, expect } from "rts:test";

const sNull: string = String(null);
const sUndef: string = String(undefined);
const sNum: string = String(42);
const sBool: string = String(true);
const sArr: string = String([1, 2, 3]);

const eqNullUndef: boolean = (null == undefined);
const eqUndefNull: boolean = (undefined == null);
const eqNullNull: boolean = (null == null);
const eqUndefUndef: boolean = (undefined == undefined);
const strictNullUndef: boolean = (null === undefined as any);

describe("String() coerce + null/undef equality (#643/#JS-spec)", () => {
  test("String(null) = 'null'", () => expect(sNull).toBe("null"));
  test("String(undefined) = 'undefined'", () => expect(sUndef).toBe("undefined"));
  test("String(42) = '42'", () => expect(sNum).toBe("42"));
  test("String(true) = 'true'", () => expect(sBool).toBe("true"));
  test("String([1,2,3]) = '1,2,3'", () => expect(sArr).toBe("1,2,3"));
  test("null == undefined", () => expect(eqNullUndef).toBe(true));
  test("undefined == null", () => expect(eqUndefNull).toBe(true));
  test("null == null", () => expect(eqNullNull).toBe(true));
  test("undefined == undefined", () => expect(eqUndefUndef).toBe(true));
  test("null === undefined eh false", () => expect(strictNullUndef).toBe(false));
});
