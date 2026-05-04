import { describe, test, expect } from "rts:test";

// instanceof para global classes (Array, Date, RegExp, Map, Set, Error*).
// Antes: warning "is not a known class" silenciava resultado.

const arr_isArr = [1,2] instanceof Array;
const arr_isObj = [1,2] instanceof Object;
const str_isArr = ("x" as any) instanceof Array;

const d_isDate = new Date(0) instanceof Date;
const re_isRe = new RegExp("x") instanceof RegExp;

const m_isMap = new Map() instanceof Map;
const s_isSet = new Set() instanceof Set;

const e = new Error("test");
const e_isErr = e instanceof Error;
const te = new TypeError("t");
const te_isTE = te instanceof TypeError;
const te_isErr = te instanceof Error;

const num_isNum = (1 as any) instanceof Number;
const bool_isBool = (true as any) instanceof Boolean;

describe("instanceof_globals", () => {
  test("[] instanceof Array", () => expect(arr_isArr).toBe(true));
  test("[] instanceof Object", () => expect(arr_isObj).toBe(true));
  test("'x' instanceof Array = false", () => expect(str_isArr).toBe(false));
  test("Date instance", () => expect(d_isDate).toBe(true));
  test("RegExp instance", () => expect(re_isRe).toBe(true));
  test("Map instance", () => expect(m_isMap).toBe(true));
  test("Set instance", () => expect(s_isSet).toBe(true));
  test("Error instance", () => expect(e_isErr).toBe(true));
  test("TypeError instance", () => expect(te_isTE).toBe(true));
  test("TypeError instanceof Error (subclass)", () => expect(te_isErr).toBe(true));
  test("primitive number not instanceof Number", () => expect(num_isNum).toBe(false));
  test("primitive bool not instanceof Boolean", () => expect(bool_isBool).toBe(false));
});
