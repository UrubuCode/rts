import { describe, test, expect } from "rts:test";

// JS spec ToString para Number: exponential quando abs >= 1e21 ou < 1e-6.
// Number.MIN_VALUE = 5e-324 (subnormal), nao MIN_POSITIVE.

const a = String(1e100);
const b = String(1e-10);
const c = String(3.14);
const d = String(0);
const e = String(Number.MIN_VALUE);
const f = String(Number.MAX_VALUE);

describe("number_format_jsspec", () => {
  test("1e100 = '1e+100'", () => expect(a).toBe("1e+100"));
  test("1e-10 = '1e-10'", () => expect(b).toBe("1e-10"));
  test("3.14 = '3.14'", () => expect(c).toBe("3.14"));
  test("0 = '0'", () => expect(d).toBe("0"));
  test("MIN_VALUE = '5e-324'", () => expect(e).toBe("5e-324"));
  test("MAX_VALUE = '1.7976931348623157e+308'", () => expect(f).toBe("1.7976931348623157e+308"));
});
