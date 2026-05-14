import { describe, test, expect } from "rts:test";

// Boolean(x) coercion edge cases
const c_empty = Boolean("");
const c_nonempty = Boolean("hello");
const c_zero = Boolean(0);
const c_one = Boolean(1);
const c_null = Boolean(null);
const c_undef = Boolean(undefined);

// new Boolean(x) wrapper
const b_true = new Boolean(true);
const b_false = new Boolean(false);
const b_one = new Boolean(1);
const b_zero = new Boolean(0);
const b_str = new Boolean("x");
const b_empty = new Boolean("");

const vot_true = b_true.valueOf();
const vot_false = b_false.valueOf();
const vot_one = b_one.valueOf();
const vot_zero = b_zero.valueOf();
const vot_str = b_str.valueOf();
const vot_empty = b_empty.valueOf();

const tos_true = b_true.toString();
const tos_false = b_false.toString();

let out: string = "";
function print(v: string): void { out += v + "\n"; }
print("coerce_empty=" + c_empty);
print("coerce_nonempty=" + c_nonempty);
print("coerce_zero=" + c_zero);
print("coerce_one=" + c_one);
print("coerce_null=" + c_null);
print("coerce_undef=" + c_undef);
print("wrap_true=" + vot_true);
print("wrap_false=" + vot_false);
print("wrap_str=" + vot_str);
print("wrap_empty=" + vot_empty);
print("ts_true=" + tos_true);
print("ts_false=" + tos_false);

describe("boolean_constructor (#784)", () => {
  test("Boolean('') is false", () => expect(c_empty).toBe(false));
  test("Boolean('hello') is true", () => expect(c_nonempty).toBe(true));
  test("Boolean(0) is false", () => expect(c_zero).toBe(false));
  test("Boolean(1) is true", () => expect(c_one).toBe(true));
  test("Boolean(null) is false", () => expect(c_null).toBe(false));
  test("Boolean(undefined) is false", () => expect(c_undef).toBe(false));
  test("new Boolean(true).valueOf() is true", () => expect(vot_true).toBe(true));
  test("new Boolean(false).valueOf() is false", () => expect(vot_false).toBe(false));
  test("new Boolean(1).valueOf() is true", () => expect(vot_one).toBe(true));
  test("new Boolean(0).valueOf() is false", () => expect(vot_zero).toBe(false));
  test("new Boolean('x').valueOf() is true", () => expect(vot_str).toBe(true));
  test("new Boolean('').valueOf() is false", () => expect(vot_empty).toBe(false));
  test("new Boolean(true).toString() is 'true'", () => expect(tos_true).toBe("true"));
  test("new Boolean(false).toString() is 'false'", () => expect(tos_false).toBe("false"));
  // Note: bool concat com string via "x=" + bool ainda emite 0/1 no print
  // helper (ortogonal a este fix; template literal sim formata "true"/"false").
});
