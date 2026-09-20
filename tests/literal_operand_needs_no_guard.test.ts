// A binary operator with a non-numeric LITERAL on one side cannot take the
// numeric fast path: `"n"` is a string in every run, so the guard that asks
// whether it is a double is a branch that always goes one way, and the
// `FloatArith` behind it is unreachable. The emitter stopped asking.
//
// Nothing here asserts that the guard is gone — `rts-host/tests/
// literal_guard_gate.rs` counts guards in the IR. These are the answers, every
// one checked against node, and this file passes on a binary from before the
// change as well as after. That is what makes it a test of the language rather
// than of the emitter.
import { describe, test, expect } from "rts:test";

describe("a string literal beside a value still concatenates", () => {
  test("either side, and the order is kept", () => {
    const i = 1;
    expect("n" + i).toBe("n1");
    expect(i + "n").toBe("1n");
    expect("a" + "b" + i).toBe("ab1");
    expect(i + 1 + "x").toBe("2x");
    expect("x" + (i + 1)).toBe("x2");
  });

  test("the value's own conversion still runs, in the right order", () => {
    const steps: string[] = [];
    const hooked = {
      valueOf() {
        steps.push("valueOf");
        return 7;
      },
      toString() {
        steps.push("toString");
        return "seven";
      },
    };
    expect("n" + hooked).toBe("n7");
    expect(steps.join()).toBe("valueOf", "the default hint asks valueOf first");
  });

  test("a value that throws while converting still throws", () => {
    const bad = {
      valueOf() {
        throw new RangeError("no");
      },
    };
    let seen = "";
    try {
      const joined = "n" + bad;
      seen = joined;
    } catch (error) {
      seen = (error as Error).message;
    }
    expect(seen).toBe("no");
  });
});

describe("every other non-numeric literal answers what it always did", () => {
  test("booleans, null and undefined coerce as the language says", () => {
    const i = 1;
    expect(true + i).toBe(2);
    expect(false + i).toBe(1);
    expect(null + i).toBe(1);
    expect(undefined + i).toBe(NaN);
    expect(true + "x").toBe("truex");
    expect(null + "x").toBe("nullx");
  });

  test("a regular expression is an object and concatenates by toString", () => {
    expect(/x/ + "!").toBe("/x/!");
    expect("" + /a+b/gi).toBe("/a+b/gi");
  });

  test("a bigint adds a bigint and refuses a number", () => {
    expect(1n + 2n).toBe(3n);
    let raised = "nothing";
    try {
      // @ts-expect-error — the language throws here, which is the point
      const mixed = 1n + 1;
      raised = String(mixed);
    } catch (error) {
      raised = (error as Error).constructor.name;
    }
    expect(raised).toBe("TypeError");
  });
});

describe("a number literal keeps the fast path it always had", () => {
  test("arithmetic and comparison against a literal", () => {
    let total = 0;
    for (let i = 0; i < 10; i++) total = total + 2 * i - 1;
    expect(total).toBe(80);
    expect(5 + 1).toBe(6);
    expect(2 ** 10).toBe(1024);
    expect(7 % 4).toBe(3);
    expect(1 / 0).toBe(Infinity);
  });
});

describe("comparisons and equalities against a literal", () => {
  test("relational operators over text compare text", () => {
    expect("a" < "b").toBe(true);
    expect("b" < "a").toBe(false);
    expect("10" < "9").toBe(true, "text order, not numeric");
    expect("2" < 10).toBe(true, "one side is text, so both convert to numbers");
  });

  test("strict and loose equality", () => {
    const value: unknown = "abc";
    expect(value === "abc").toBe(true);
    expect(value !== "abd").toBe(true);
    expect(null == undefined).toBe(true);
    expect(null === undefined).toBe(false);
    expect(0 == "").toBe(true);
    expect(0 === ("" as unknown)).toBe(false);
    expect(NaN === NaN).toBe(false);
  });

  test("a literal on the left of a loop's test", () => {
    let seen = 0;
    for (let i = 0; 3 > i; i++) seen++;
    expect(seen).toBe(3);
  });
});

describe("the operand is still evaluated exactly once, and in source order", () => {
  test("a side effect on the side that is not the literal", () => {
    let calls = 0;
    const next = () => {
      calls++;
      return calls;
    };
    expect("n" + next()).toBe("n1");
    expect(calls).toBe(1);
    expect(next() + "n").toBe("2n");
    expect(calls).toBe(2);
  });

  test("both sides of a compound assignment", () => {
    let text = "a";
    text += "b";
    text += 1;
    expect(text).toBe("ab1");
    let count = 1;
    count += 1;
    expect(count).toBe(2);
  });
});
