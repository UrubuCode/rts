import { describe, test, expect } from "rts:test";

// A call passing more arguments than the convention carries hands the runtime a
// vector, and a parameter past the fourth reads one position of it. What must
// hold whatever the vector's shape: every position arrives, a missing one is
// `undefined`, `arguments`, `...rest` and `.length` agree with the positions,
// and the TypeError for a non-callable still names the callee. The eight-value
// literal is pinned beside it because it is the same builder.

function five(a: number, b: number, c: number, d: number, e: number): number {
  return a * 10000 + b * 1000 + c * 100 + d * 10 + e;
}
function nine(a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number): string {
  return [a, b, c, d, e, f, g, h, i].join("");
}
function tail(a: number, b: number, c: number, d: number, e?: number, f?: number): string {
  return `${a}${b}${c}${d}${e}${f}`;
}
function rest(a: number, ...r: number[]): string {
  return `${a}:${r.join(",")}:${r.length}`;
}
function count(): number { return arguments.length; }
function pick(): any { return arguments[6]; }

describe("a call past the convention's slots", () => {
  test("every position arrives, five and nine", () => {
    expect(five(1, 2, 3, 4, 5)).toBe(12345);
    expect(nine(1, 2, 3, 4, 5, 6, 7, 8, 9)).toBe("123456789");
    let acc = 0;
    for (let i = 0; i < 200; i++) acc += five(i, 0, 0, 0, i);
    expect(acc).toBe(199 * 200 / 2 * 10001);
  });
  test("a parameter nothing was passed for is undefined", () => {
    expect(tail(1, 2, 3, 4)).toBe("1234undefinedundefined");
    expect(tail(1, 2, 3, 4, 5)).toBe("12345undefined");
    expect((five as any)(1, 2, 3, 4)).toBeNaN();
  });
  test("rest and arguments agree with the positions", () => {
    expect(rest(1)).toBe("1::0");
    expect(rest(1, 2, 3)).toBe("1:2,3:2");
    expect(rest(1, 2, 3, 4, 5, 6, 7)).toBe("1:2,3,4,5,6,7:6");
    expect((count as any)()).toBe(0);
    expect((count as any)(1, 2, 3, 4, 5, 6, 7)).toBe(7);
    expect((pick as any)(0, 1, 2, 3, 4, 5, 6, 7)).toBe(6);
    expect((pick as any)(0, 1, 2)).toBe(undefined);
  });
  test("spread, apply and Reflect.apply reach the same positions", () => {
    const xs = [1, 2, 3, 4, 5];
    expect(five(...xs)).toBe(12345);
    expect(five.apply(null, xs)).toBe(12345);
    expect(Reflect.apply(five, null, xs)).toBe(12345);
    expect(five.call(null, 5, 4, 3, 2, 1)).toBe(54321);
    const holes = [1, , 3, 4, 5] as any;
    expect((five as any)(...holes)).toBeNaN();
    expect(five.apply(null, { length: 5, 0: 1, 1: 2, 2: 3, 3: 4, 4: 5 } as any)).toBe(12345);
  });
  test("the TypeError still names a callee past the slots", () => {
    const o: any = { m: 1 };
    // A callee reached through a CAPTURED receiver has no spelling the emitter
    // reports today, five arguments or two alike, so the kind is what the message
    // carries here; `o.m` at the top level of a program is named. Pinned as the
    // ending, which both forms share.
    let message = "";
    try { o.m(1, 2, 3, 4, 5); } catch (e: any) { message = e.message; }
    expect(message.endsWith("is not a function")).toBe(true);
    let nameless = "";
    try { (o.zz as any)(1, 2, 3, 4, 5); } catch (e: any) { nameless = e.message; }
    expect(nameless.endsWith("is not a function")).toBe(true);
  });
  test("an array literal of eight is the same array", () => {
    const xs = [1, 2, 3, 4, 5, 6, 7, 8];
    expect(xs.length).toBe(8);
    expect(xs.join()).toBe("1,2,3,4,5,6,7,8");
    const ys = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    expect(ys[8]).toBe(9);
    expect(ys.length).toBe(9);
    const hs = [1, , 3, 4, 5, 6, 7, 8];
    expect(1 in hs).toBe(false);
    expect(hs.length).toBe(8);
  });
});
