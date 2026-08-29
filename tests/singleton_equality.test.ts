// `x === undefined` and `x === null` as a bit test rather than a runtime call.
//
// Every assertion here is about a VALUE. The emission is what changed, and a
// test that asserted the emission would pass on a build that answered wrongly —
// so the cases are chosen to be the ones a bit test could get wrong: values
// that are nearly a singleton, a shadowed `undefined`, both operand orders, and
// the proven operands for which the machine refuses the question outright.
import { describe, test, expect } from "rts:test";

describe("equality against a singleton", () => {
  test("undefined on either side, and the values near it", () => {
    let missing: any;
    expect(missing === undefined).toBe(true);
    expect(undefined === missing).toBe(true);
    expect(missing !== undefined).toBe(false);
    for (const near of [null, 0, "", false, NaN, [], {}, "undefined"]) {
      expect(near === undefined).toBe(false);
      expect(near !== undefined).toBe(true);
      expect(undefined === near).toBe(false);
    }
  });

  test("null on either side, and null is not undefined", () => {
    const nothing: any = null;
    expect(nothing === null).toBe(true);
    expect(null === nothing).toBe(true);
    expect(nothing === undefined).toBe(false);
    expect(undefined === nothing).toBe(false);
    for (const near of [0, "", false, NaN, [], {}, "null"]) {
      expect(near === null).toBe(false);
      expect(near !== null).toBe(true);
    }
  });

  test("a PROVEN operand is not a singleton and the answer is constant", () => {
    // The arithmetic proves a double, so nothing at run time is asked: the
    // machine refuses `IsSingleton` for a proven operand rather than emitting
    // a test that is always false. The answers still have to be these.
    const n = 1 + 1;
    expect(n === undefined).toBe(false);
    expect(n !== undefined).toBe(true);
    expect(n === null).toBe(false);
    expect(0 === undefined).toBe(false);
    expect(NaN === undefined).toBe(false);
  });

  test("a SHADOWED undefined is compared as the binding, not the singleton", () => {
    // The reason this is decided on the operand and not on the syntax. Here
    // `undefined` is an ordinary local holding 5, so `x === undefined` asks
    // about 5 — and a version that recognised the NAME would answer the
    // singleton's question and be wrong.
    function shadowed(): string {
      const undefined = 5;
      const five: any = 5;
      let nothing: any;
      return String(five === undefined) + "," + String(nothing === undefined);
    }
    expect(shadowed()).toBe("true,false");
  });

  test("a defaulted parameter takes its default only for undefined", () => {
    function withDefault(x: number, y: number = 7): number {
      return x * 100 + y;
    }
    expect(withDefault(1)).toBe(107);
    expect(withDefault(1, undefined)).toBe(107);
    expect(withDefault(1, 2)).toBe(102);
    // `null` is NOT undefined, so it does NOT take the default — the one case
    // a nullish test would get wrong, and the reason this is not `??`.
    expect(withDefault(1, null as any)).toBe(100);
    expect(withDefault(1, 0)).toBe(100);
  });

  test("a destructuring default follows the same rule", () => {
    const { a = 9 } = {} as { a?: number };
    const { b = 9 } = { b: undefined } as { b?: number };
    const { c = 9 } = { c: null } as any;
    const { d = 9 } = { d: 0 };
    expect(a).toBe(9);
    expect(b).toBe(9);
    expect(c).toBe(null);
    expect(d).toBe(0);
    const [p = 1, q = 2] = [undefined, 5] as (number | undefined)[];
    expect(p).toBe(1);
    expect(q).toBe(5);
  });

  test("a boxed value is never a singleton", () => {
    // Two objects with the same shape are not `===`, and neither is either of
    // them `=== null` — a word comparison must not confuse a reference payload
    // with a singleton's number.
    const one: any = { v: 1 };
    const two: any = { v: 1 };
    expect(one === two).toBe(false);
    expect(one === null).toBe(false);
    expect(one === undefined).toBe(false);
    expect(one === one).toBe(true);
    const held = [null, undefined, one];
    expect(held.indexOf(null)).toBe(0);
    expect(held.indexOf(undefined)).toBe(1);
  });

  test("the comparison in a condition and as a value agree", () => {
    let missing: any;
    const asValue = missing === undefined;
    let asCondition = false;
    if (missing === undefined) asCondition = true;
    expect(asValue).toBe(asCondition);
    expect(typeof asValue).toBe("boolean");
    expect(String(missing === undefined)).toBe("true");
    expect([missing === undefined, missing !== undefined].join(",")).toBe("true,false");
  });
});
