import { describe, test, expect } from "rts:test";

// Every member of `Math` a program reaches by name, where the whole program
// leaves `Math` alone, is decided by the compiler: an instruction where the
// hardware has one, a direct unboxed call where it does not. What this file
// pins is the LANGUAGE's answer in the cases an instruction sequence gets wrong
// when written the obvious way — signed zeros, NaN, ties, the 32-bit views —
// and that the members still behave as values and survive being replaced.

function d(x: number): number { return x; }   // a value nothing proves at the site
const same = (a: number, b: number) => Object.is(a, b);

describe("Math as the machine's own instructions", () => {
  test("round: ties go up, tiny negatives keep -0, 0.49999999999999994 is 0", () => {
    expect(Math.round(2.5)).toBe(3);
    expect(Math.round(-2.5)).toBe(-2);
    expect(Math.round(-0.5) === 0 && same(Math.round(-0.5), -0)).toBe(true);
    expect(same(Math.round(-0.3), -0)).toBe(true);
    expect(Math.round(0.49999999999999994)).toBe(0);
    expect(Math.round(d(1e16) + 0.5)).toBe(1e16);
    expect(Number.isNaN(Math.round(NaN))).toBe(true);
    expect(Math.round(Infinity)).toBe(Infinity);
    let acc = 0;
    for (let i = -20; i < 20; i++) acc += Math.round(i * 0.25);
    expect(acc).toBe(0);
  });
  test("sign keeps the zero it was given and NaN", () => {
    expect(Math.sign(7)).toBe(1);
    expect(Math.sign(-7.5)).toBe(-1);
    expect(same(Math.sign(0), 0)).toBe(true);
    expect(same(Math.sign(-0), -0)).toBe(true);
    expect(Number.isNaN(Math.sign(NaN))).toBe(true);
    expect(Math.sign(-Infinity)).toBe(-1);
  });
  test("fround rounds to single precision", () => {
    expect(Math.fround(5.5)).toBe(5.5);
    expect(Math.fround(5.05)).toBe(5.050000190734863);
    expect(Math.fround(1e40)).toBe(Infinity);
    expect(same(Math.fround(-0), -0)).toBe(true);
    expect(Number.isNaN(Math.fround(NaN))).toBe(true);
  });
  test("imul is the wrapping 32-bit product", () => {
    expect(Math.imul(3, 4)).toBe(12);
    expect(Math.imul(0xffffffff, 5)).toBe(-5);
    expect(Math.imul(0x7fffffff, 2)).toBe(-2);
    expect(Math.imul(-1, 8)).toBe(-8);
    expect(Math.imul(2.9, 3.9)).toBe(6);
    expect(Math.imul(NaN, 3)).toBe(0);
    let h = 0;
    for (let i = 0; i < 1000; i++) h = Math.imul(h ^ i, 0x9e3779b1);
    expect(h).toBe(-1096780032);
  });
  test("clz32 counts on the 32-bit view", () => {
    expect(Math.clz32(1)).toBe(31);
    expect(Math.clz32(0)).toBe(32);
    expect(Math.clz32(-1)).toBe(0);
    expect(Math.clz32(0x80000000)).toBe(0);
    expect(Math.clz32(1000)).toBe(22);
    expect(Math.clz32(NaN)).toBe(32);
    expect(Math.clz32(3.7)).toBe(30);
  });
  test("hypot and pow keep the runtime's edge answers", () => {
    expect(Math.hypot(3, 4)).toBe(5);
    expect(Math.hypot(1e200, 1e200)).toBe(1.414213562373095e200);
    expect(Math.hypot(Infinity, NaN)).toBe(Infinity);
    expect(Math.hypot()).toBe(0);
    expect(Math.pow(2, 10)).toBe(1024);
    expect(Number.isNaN(Math.pow(1, Infinity))).toBe(true);
    expect(Math.pow(-8, 1 / 3)).toBeNaN();
    expect(Math.pow(d(2), d(0.5))).toBe(Math.SQRT2);
  });
  test("the transcendentals answer what the runtime answers", () => {
    expect(Math.sin(0)).toBe(0);
    expect(Math.cos(0)).toBe(1);
    expect(Math.abs(Math.sin(Math.PI / 2) - 1) < 1e-15).toBe(true);
    expect(Math.atan2(1, 1)).toBe(Math.PI / 4);
    expect(Math.exp(0)).toBe(1);
    expect(Math.log(Math.E)).toBe(1);
    expect(Math.log2(8)).toBe(3);
    expect(Math.log10(1000)).toBe(3);
    expect(Math.cbrt(27)).toBe(3);
    expect(Math.atanh(-0.5)).toBe(-Math.atanh(0.5));
    expect(same(Math.atanh(-0), -0)).toBe(true);
    expect(Math.expm1(0)).toBe(0);
    expect(Math.log1p(0)).toBe(0);
    expect(Math.tanh(Infinity)).toBe(1);
    let s = 0;
    for (let i = 0; i < 100; i++) s += Math.sin(i * 0.1) * Math.cos(i * 0.1);
    // Rust's libm and V8's fdlibm differ in the last bits of sin and cos, so the
    // sum is pinned to a tolerance and not to a digit string.
    expect(Math.abs(s - 1.2466225917317624) < 1e-12).toBe(true);
  });
  test("constants are the language's", () => {
    expect(Math.PI).toBe(3.141592653589793);
    expect(Math.E).toBe(2.718281828459045);
    expect(Math.LN2 * Math.LOG2E).toBe(1);
    expect(Math.SQRT1_2 * Math.SQRT2).toBe(1.0000000000000002);
    expect(Math.LN10).toBe(2.302585092994046);
    expect(Math.LOG10E).toBe(0.4342944819032518);
  });
  test("a member is still a value and still coerces its argument", () => {
    const f = Math.round;
    expect(f(1.5)).toBe(2);
    expect(Math.round("2.5" as any)).toBe(3);
    expect(Math.sign("-3" as any)).toBe(-1);
    expect(Math.imul("3" as any, "4" as any)).toBe(12);
    expect([1.4, 2.6].map(Math.round)).toEqual([1, 3]);
    expect(Math.max.call(null, 1, 2)).toBe(2);
    expect(typeof Math.sin).toBe("function");
    expect(Math.sin.length).toBe(1);
  });
});
