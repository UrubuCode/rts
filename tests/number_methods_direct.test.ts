import { describe, test, expect } from "rts:test";

// `n.toString(radix)` and `n.toFixed(d)` over a number the compiler has proved
// compile to one entry with the double itself; over anything else — a wrapper,
// a parameter nothing proved, a patched object — the member is called. What
// this pins is that both answer the language's text: the radix rules, the
// range errors, the rounding, and a receiver that is not a plain number.

describe("number methods over a proven double", () => {
  test("toString spells base ten and other radixes", () => {
    let acc = "";
    for (let i = 250; i < 260; i++) acc += i.toString() + i.toString(16) + i.toString(2).length;
    expect(acc).toBe("250fa8" + "251fb8" + "252fc8" + "253fd8" + "254fe8" + "255ff8" + "2561009" + "2571019" + "2581029" + "2591039");
    let x = 0.5;
    expect((x + 0.25).toString()).toBe("0.75");
    expect((x * 2 - 1).toString()).toBe("0");
    expect((-x).toString(2)).toBe("-0.1");
    expect((x * 1e21).toString()).toBe("500000000000000000000");
    expect((x * 4e21).toString()).toBe("2e+21");
    expect((x / 0).toString()).toBe("Infinity");
    expect((x - x / 0).toString()).toBe("-Infinity");
    expect((x * NaN).toString()).toBe("NaN");
    expect((35 + x - x).toString(36)).toBe("z");
    expect((255.5 - x).toString(16)).toBe("ff");
  });
  test("toString refuses a radix outside 2..36", () => {
    let i = 5;
    for (const bad of [0, 1, 37, -3, 1.5 + 36]) {
      let message = "";
      try { (i + 1).toString(bad); } catch (e: any) { message = e.message; }
      expect(message.includes("radix")).toBe(true);
    }
    expect((i + 1).toString(undefined)).toBe("6");
    expect((i + 1).toString(10.9)).toBe("6");
  });
  test("toFixed rounds half away from zero and honours the range", () => {
    let acc = 0;
    for (let i = 0; i < 300; i++) acc += (i + 0.005).toFixed(2).length;
    expect(acc).toBe(10 * 4 + 90 * 5 + 200 * 6);
    let z = 0;
    expect((z + 2.5).toFixed(0)).toBe("3");
    expect((z - 2.5).toFixed(0)).toBe("-3");
    expect((z + 1.005).toFixed(2)).toBe("1.00");
    expect((z + 2.55).toFixed(1)).toBe("2.5");
    expect((z + 1e21).toFixed(2)).toBe("1e+21");
    expect((z + 1.5).toFixed()).toBe("2");
    expect((z + 1.5).toFixed("1" as any)).toBe("1.5");
    expect((z + 1.5).toFixed(NaN)).toBe("2");
    let message = "";
    try { (z + 1).toFixed(101); } catch (e: any) { message = e.message; }
    expect(message.includes("toFixed")).toBe(true);
    let negative = "";
    try { (z + 1).toFixed(-1); } catch (e: any) { negative = e.message; }
    expect(negative.includes("toFixed")).toBe(true);
    expect((z - 0).toFixed(2)).toBe("0.00");
    expect((z + NaN).toFixed(2)).toBe("NaN");
  });
  test("a receiver the compiler did not prove still takes the member", () => {
    const unknown: any = 255;
    expect(unknown.toString(16)).toBe("ff");
    expect(unknown.toFixed(1)).toBe("255.0");
    const boxed: any = new Number(7);
    expect(boxed.toString(2)).toBe("111");
    expect(boxed.toFixed(2)).toBe("7.00");
    const impostor: any = { toString: (r: number) => `r${r}`, toFixed: (d: number) => `d${d}` };
    expect(impostor.toString(3)).toBe("r3");
    expect(impostor.toFixed(4)).toBe("d4");
    const text: any = "12";
    expect(text.toString()).toBe("12");
    function viaParam(n: number): string { return n.toString(8) + n.toFixed(0); }
    expect(viaParam(64)).toBe("10064");
    let evaluated = 0;
    const order = (i: number) => { evaluated += i; return i; };
    expect(order(3).toFixed(order(1))).toBe("3.0");
    expect(evaluated).toBe(4);
  });
});
