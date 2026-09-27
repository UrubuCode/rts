import { describe, test, expect } from "rts:test";

// `Number.isNaN`, `isFinite`, `isInteger`, `isSafeInteger`, the global `isNaN`
// and `isFinite`, `Array.isArray` and `Object.is` are decided by the compiler
// where the program leaves those names alone. What this pins is the language's
// answer in the cases an instruction gets wrong when written the obvious way,
// and the difference between the converting and the non-converting predicates.

function d(x: any): any { return x; }   // a value nothing proves at the site
function n(x: number): number { return x; }

describe("the well-known predicates as instructions", () => {
  test("Number.isNaN does not convert; isNaN does", () => {
    expect(Number.isNaN(NaN)).toBe(true);
    expect(Number.isNaN(n(0) / n(0))).toBe(true);
    expect(Number.isNaN(1)).toBe(false);
    expect(Number.isNaN("abc" as any)).toBe(false);
    expect(Number.isNaN(d("abc"))).toBe(false);
    expect(Number.isNaN(undefined as any)).toBe(false);
    expect(isNaN("abc" as any)).toBe(true);
    expect(isNaN(d("12"))).toBe(false);
    expect(isNaN(n(1) / n(0))).toBe(false);
    let c = 0;
    for (let i = 0; i < 100; i++) if (Number.isNaN(i % 7 === 0 ? NaN : i)) c++;
    expect(c).toBe(15);
  });
  test("isFinite: infinities and NaN are not, everything else is", () => {
    expect(Number.isFinite(1e308)).toBe(true);
    expect(Number.isFinite(n(1) / n(0))).toBe(false);
    expect(Number.isFinite(-Infinity)).toBe(false);
    expect(Number.isFinite(NaN)).toBe(false);
    expect(Number.isFinite("5" as any)).toBe(false);
    expect(isFinite("5" as any)).toBe(true);
    expect(isFinite(d(null))).toBe(true);
    expect(isFinite(d("x"))).toBe(false);
  });
  test("isInteger: the infinities are not, -0 and 2^53 are", () => {
    expect(Number.isInteger(5)).toBe(true);
    expect(Number.isInteger(5.5)).toBe(false);
    expect(Number.isInteger(-0)).toBe(true);
    expect(Number.isInteger(n(1) / n(0))).toBe(false);
    expect(Number.isInteger(NaN)).toBe(false);
    expect(Number.isInteger(2 ** 53)).toBe(true);
    expect(Number.isInteger(1e300)).toBe(true);
    expect(Number.isInteger("5" as any)).toBe(false);
    let c = 0;
    for (let i = 0; i < 100; i++) if (Number.isInteger(i * 0.5)) c++;
    expect(c).toBe(50);
  });
  test("isSafeInteger stops at 2^53 - 1", () => {
    expect(Number.isSafeInteger(9007199254740991)).toBe(true);
    expect(Number.isSafeInteger(9007199254740992)).toBe(false);
    expect(Number.isSafeInteger(-9007199254740991)).toBe(true);
    expect(Number.isSafeInteger(1.5)).toBe(false);
    expect(Number.isSafeInteger(n(1) / n(0))).toBe(false);
    expect(Number.isSafeInteger(NaN)).toBe(false);
    expect(Number.isSafeInteger("3" as any)).toBe(false);
  });
  test("Array.isArray asks what the thing is, not what it inherits", () => {
    expect(Array.isArray([])).toBe(true);
    expect(Array.isArray([1, 2])).toBe(true);
    expect(Array.isArray({ length: 1 })).toBe(false);
    expect(Array.isArray("abc")).toBe(false);
    expect(Array.isArray(d(null))).toBe(false);
    expect(Array.isArray(undefined)).toBe(false);
    class Sub extends Array {}
    expect(Array.isArray(new Sub())).toBe(true);
    const detached = [1];
    Object.setPrototypeOf(detached, null);
    expect(Array.isArray(detached)).toBe(true);
    expect(Array.isArray(new Proxy([], {}))).toBe(true);
    let c = 0;
    const things: any[] = [[], {}, [1], "s", 1, null];
    for (let i = 0; i < 60; i++) if (Array.isArray(things[i % 6])) c++;
    expect(c).toBe(20);
  });
  test("Object.is separates the zeros and unites NaN", () => {
    expect(Object.is(0, -0)).toBe(false);
    expect(Object.is(-0, -0)).toBe(true);
    expect(Object.is(NaN, NaN)).toBe(true);
    expect(Object.is(NaN, n(0) / n(0))).toBe(true);
    expect(Object.is(1, 1)).toBe(true);
    expect(Object.is("a", "a")).toBe(true);
    expect(Object.is("a", d("b"))).toBe(false);
    const o = {};
    expect(Object.is(o, o)).toBe(true);
    expect(Object.is(o, {})).toBe(false);
    expect(Object.is(1n, 1n)).toBe(true);
    expect(Object.is(1n, 2n)).toBe(false);
    expect(Object.is(d(1n), d(1n))).toBe(true);
    expect(Object.is(null, undefined)).toBe(false);
  });
  test("the members are still values", () => {
    const f = Number.isInteger;
    expect(f(3)).toBe(true);
    expect([1, 1.5, 2].filter(Number.isInteger)).toEqual([1, 2]);
    expect([[], 1].map(Array.isArray)).toEqual([true, false]);
    expect(typeof Object.is).toBe("function");
  });
});
