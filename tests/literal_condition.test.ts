// A condition that is a LITERAL is decided while it is still one.
//
// `expr::constant` stamps UNPROVEN on every constant it materialises, so the
// truth of `while (true)` had to be recovered by a call to `__rts_to_boolean`
// on every iteration of the loop it controls. The falsy rule is the LANGUAGE's,
// so it is decided in the emitter rather than asked of the machine.
//
// Every expectation was checked against node first.
import { describe, test, expect } from "rts:test";

describe("a literal condition", () => {
  test("the falsy literals are false and nothing else is", () => {
    const out: string[] = [];
    if (true) out.push("T"); else out.push("f");
    if (false) out.push("F"); else out.push("t");
    if (1) out.push("1"); else out.push(".");
    if (0) out.push("0"); else out.push(".");
    if (-0) out.push("-0"); else out.push(".");
    if (NaN) out.push("N"); else out.push(".");
    if ("a") out.push("s"); else out.push(".");
    if ("") out.push("e"); else out.push(".");
    if ("0") out.push("z"); else out.push(".");
    if (null) out.push("n"); else out.push(".");
    if (undefined) out.push("u"); else out.push(".");
    if (/x/) out.push("r"); else out.push(".");
    if (1n) out.push("b"); else out.push(".");
    if (0n) out.push("B"); else out.push(".");
    if (1e-323) out.push("m"); else out.push(".");
    expect(out.join("")).toBe("Tt1...s.z..rb.m");
  });

  test("the three loop heads that can carry one", () => {
    let i = 0;
    while (true) {
      i++;
      if (i >= 3) break;
    }
    expect(i).toBe(3);

    let j = 0;
    do {
      j++;
    } while (false);
    expect(j).toBe(1);

    let k = 0;
    for (; true; ) {
      k++;
      if (k >= 2) break;
    }
    expect(k).toBe(2);
  });

  test("a false literal head runs the body zero times", () => {
    let ran = 0;
    while (false) {
      ran++;
    }
    expect(ran).toBe(0);
    let forRan = 0;
    for (; false; ) {
      forRan++;
    }
    expect(forRan).toBe(0);
    // `do` runs once whatever its test says, which is what makes it different.
    let once = 0;
    do {
      once++;
    } while (false);
    expect(once).toBe(1);
  });

  test("a literal in a ternary and in a short circuit", () => {
    expect(true ? "a" : "b").toBe("a");
    expect(false ? "a" : "b").toBe("b");
    expect(0 ? "a" : "b").toBe("b");
    expect("" ? "a" : "b").toBe("b");
    expect(null ? "a" : "b").toBe("b");
    expect((0 && 1) === 0).toBe(true);
    expect((1 && 2) === 2).toBe(true);
    expect((0 || 3) === 3).toBe(true);
    expect(("" || "x") === "x").toBe(true);
  });

  test("a bigint zero written any way is still false", () => {
    if (0n) {
      expect(true).toBe(false);
    }
    if (1n) {
      expect(true).toBe(true);
    }
    expect(0n ? "a" : "b").toBe("b");
    expect(10n ? "a" : "b").toBe("a");
  });

  test("a NON-literal condition is unchanged", () => {
    const held: any = 0;
    const truthy: any = "x";
    let taken = "";
    if (held) taken += "1";
    if (truthy) taken += "2";
    if (!held) taken += "3";
    expect(taken).toBe("23");
    let n = 0;
    while (n < 3) n++;
    expect(n).toBe(3);
  });
});
