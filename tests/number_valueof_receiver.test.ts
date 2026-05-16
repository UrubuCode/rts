import { describe, test, expect } from "rts:test";

// (#245) Number prototype methods em receiver Number-typed (de
// `new Number(x)`) ABI espera F64 no slot do receiver. Antes, o caller
// (lower_global_instance_call) sempre passava i64 raw (via coerce_to_i64
// que faz fcvt em F64), causando Verifier error
// `arg has type i64, expected f64`. Fix: detectar member.args[0] == F64
// e usar coerce_to_f64.

const n = new Number(42);
const v = n.valueOf();
const z = new Number(0).valueOf();
const m = new Number(7.5).valueOf();
const t = (123).toString();
const t2 = (3.14).toFixed(1);

describe("Number instance methods em receiver Number (#245)", () => {
  test("new Number(42).valueOf() === 42", () => expect(v).toBe(42));
  test("new Number(0).valueOf() === 0", () => expect(z).toBe(0));
  test("new Number(7.5).valueOf() === 7.5", () => expect(m).toBe(7.5));
  test("(123).toString() === '123' (regressao primitivo)", () =>
    expect(t).toBe("123"));
  test("(3.14).toFixed(1) === '3.1'", () => expect(t2).toBe("3.1"));
});
