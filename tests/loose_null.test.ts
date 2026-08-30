// `x == null` as the nullish test it is, rather than a call to loose equality.
//
// The whole risk of this change is that `==` is a coercing operator and this
// arm is the one that does not coerce. So the cases below are chosen to be the
// values that `==` DOES relate to something — `0 == false`, `"" == 0`,
// `[] == ""` are all true in JavaScript — and none of them may become loosely
// equal to `null`.
import { describe, test, expect } from "rts:test";

describe("loose equality against null and undefined", () => {
  test("exactly null and undefined are loosely null", () => {
    let missing: any;
    const nothing: any = null;
    expect(nothing == null).toBe(true);
    expect(missing == null).toBe(true);
    expect(nothing == undefined).toBe(true);
    expect(missing == undefined).toBe(true);
    expect(null == undefined).toBe(true);
    expect(undefined == null).toBe(true);
  });

  test("the coercible values are NOT, which is the whole hazard", () => {
    // Each of these is loosely equal to something — `0 == false` is true,
    // `"" == 0` is true, `[] == ""` is true — and none of them is `== null`.
    for (const near of [0, -0, "", "0", false, NaN, [], {}, "null", "undefined", 0n]) {
      expect(near == null).toBe(false);
      expect(near != null).toBe(true);
      expect(null == near).toBe(false);
      expect(near == undefined).toBe(false);
    }
    // And the loose relations that DO hold still hold, so nothing was traded.
    expect(0 == (false as any)).toBe(true);
    expect(("" as any) == 0).toBe(true);
    expect(([] as any) == "").toBe(true);
    expect(("1" as any) == 1).toBe(true);
  });

  test("both operand orders, and the negation", () => {
    let missing: any;
    const held: any = 5;
    expect(missing != null).toBe(false);
    expect(null != missing).toBe(false);
    expect(held != null).toBe(true);
    expect(null != held).toBe(true);
    expect(undefined != held).toBe(true);
  });

  test("loose is not strict: null == undefined but null !== undefined", () => {
    const nothing: any = null;
    let missing: any;
    expect(nothing == missing).toBe(true);
    expect(nothing === missing).toBe(false);
    expect(nothing != missing).toBe(false);
    expect(nothing !== missing).toBe(true);
    // `== null` catches both; `=== null` catches one. The distinction is the
    // reason the idiom exists at all.
    expect(missing == null).toBe(true);
    expect(missing === null).toBe(false);
  });

  test("a PROVEN operand is never nullish and the answer is constant", () => {
    const n = 1 + 1;
    expect(n == null).toBe(false);
    expect(n != null).toBe(true);
    expect(n == undefined).toBe(false);
    expect(0 == null).toBe(false);
    expect(NaN == null).toBe(false);
  });

  test("a SHADOWED undefined is compared as the binding", () => {
    function shadowed(): string {
      const undefined = 0;
      let missing: any;
      const zero: any = 0;
      // `zero == undefined` here is `0 == 0`, which is true. A version that
      // recognised the NAME rather than the value would answer the nullish
      // question and say false.
      return String(zero == undefined) + "," + String(missing == undefined);
    }
    expect(shadowed()).toBe("true,false");
  });

  test("as a condition, as a value, and guarding a property read", () => {
    const values: any[] = [null, undefined, 0, "", { v: 1 }];
    let nullish = 0;
    for (const v of values) if (v == null) nullish++;
    expect(nullish).toBe(2);
    const flags = values.map((v) => v == null);
    expect(flags.join(",")).toBe("true,true,false,false,false");
    // The idiom in the shape it is actually written.
    function nameOf(o: any): string {
      if (o == null) return "none";
      return String(o.v);
    }
    expect(nameOf(null)).toBe("none");
    expect(nameOf(undefined)).toBe("none");
    expect(nameOf({ v: 7 })).toBe("7");
  });

  test("the operand is evaluated exactly once", () => {
    const log: string[] = [];
    function make(tag: string, value: any): any {
      log.push(tag);
      return value;
    }
    expect(make("a", null) == null).toBe(true);
    expect(log.join(",")).toBe("a");
    expect(make("b", 1) != null).toBe(true);
    expect(log.join(",")).toBe("a,b");
  });

  test("an object with valueOf is not consulted, because null coerces nothing", () => {
    // `==` against a number WOULD run this. Against `null` the specification
    // never reaches the ToPrimitive arm, so a build that called loose equality
    // and a build that tests two singletons must agree — and the counter is
    // what says nothing ran.
    let calls = 0;
    const tricky = {
      valueOf(): number {
        calls++;
        return 0;
      },
    };
    expect((tricky as any) == null).toBe(false);
    expect((tricky as any) != null).toBe(true);
    expect(calls).toBe(0);
    // And it IS consulted where the language says so.
    expect((tricky as any) == 0).toBe(true);
    expect(calls).toBe(1);
  });
});

describe("the runtime's own loose equality, reached without a literal", () => {
  // These reach `__rts_loose_equals` rather than the emitter's settled form,
  // because the callee is declared twice in the program and the inliner refuses
  // both — which is what makes them a test of the RUNTIME's arm ordering and
  // not of the emitter's recognition.
  function compare(left: any, right: any): boolean {
    return left == right;
  }
  function elsewhere(): (l: any, r: any) => boolean {
    function compare(left: any, right: any): boolean {
      return left == right;
    }
    return compare;
  }
  const refused = elsewhere();

  test("a conversion is NOT run against null or undefined", () => {
    // The specification's steps 2 to 4 come before step 10's ToPrimitive. This
    // ran the conversion first and then discovered the null: `valueOf` was
    // called twice per comparison where node calls it zero times, so a program
    // whose conversion counts or logs behaved differently here.
    let calls = 0;
    const counting = {
      valueOf(): number {
        calls++;
        return 0;
      },
      toString(): string {
        calls++;
        return "0";
      },
    };
    expect(refused(counting, null)).toBe(false);
    expect(calls).toBe(0);
    expect(refused(counting, undefined)).toBe(false);
    expect(calls).toBe(0);
    expect(refused(null, counting)).toBe(false);
    expect(calls).toBe(0);
    // And the conversion IS run where the language says so, which is the half
    // that must not be traded away for the half above.
    expect(refused(counting, 0)).toBe(true);
    expect(calls).toBe(1);
  });

  test("a valueOf that answers undefined still compares as undefined", () => {
    // Why the same rule is asked TWICE: `ToPrimitive` can produce `undefined`,
    // and the specification re-enters the comparison with the converted value
    // rather than continuing down the table.
    const nothing = {
      valueOf(): any {
        return undefined;
      },
      toString(): any {
        return undefined;
      },
    };
    expect(refused(nothing, 0)).toBe(false);
    expect(refused(nothing, "")).toBe(false);
    expect(refused(nothing, undefined)).toBe(false);
    expect(refused(nothing, null)).toBe(false);
  });

  test("everything else loose equality does is unchanged", () => {
    expect(compare(0, false)).toBe(true);
    expect(compare("", 0)).toBe(true);
    expect(compare("1", 1)).toBe(true);
    expect(compare([], "")).toBe(true);
    expect(compare([1], 1)).toBe(true);
    expect(compare({}, {})).toBe(false);
    expect(compare(1n, 1)).toBe(true);
    expect(compare(NaN, NaN)).toBe(false);
    expect(compare(null, undefined)).toBe(true);
    expect(compare(null, 0)).toBe(false);
    expect(compare(undefined, 0)).toBe(false);
    expect(compare(null, false)).toBe(false);
  });
});
