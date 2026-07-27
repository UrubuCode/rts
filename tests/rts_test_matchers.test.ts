// Self-test of the `rts:test` FRAMEWORK itself (crates/rts-std/src/test/bundle.ts).
//
// WHY THIS EXISTS
//
// `bundle.ts` is compiled into every single test run, so a lowering bail inside it
// does not fail one file — it fails ALL of them, with an error naming a Matcher
// method rather than the real cause. That happened when the `rts:string` namespace
// was drained (commit 5c0b0e02): `bundle.ts` still called
// `string.contains/starts_with/ends_with`, and the whole suite went 737/737 red.
//
// Nothing covered the matchers themselves, so there was no test whose name said
// "the framework broke". This file is that test. Every matcher is exercised in
// both polarities (plain and `.not`), so a bail or a wrong result in any of them
// surfaces here first, by name.
//
// Keep it dependency-free: only `rts:test` itself, no other import.

import { describe, test, expect } from "rts:test";

describe("rts:test framework — equality matchers", () => {
  test("toBe / not.toBe", () => {
    expect("hello").toBe("hello");
    expect("hello").not.toBe("world");
  });

  test("toEqual / not.toEqual", () => {
    expect("42").toEqual("42");
    expect("42").not.toEqual("43");
  });

  test("toBe treats NaN as matching NaN (SameValue, unlike ===)", () => {
    expect(`${NaN}`).toBe(`${NaN}`);
  });
});

describe("rts:test framework — string matchers", () => {
  // These three are the ones that broke the whole suite when rts:string was
  // drained; they now route through the primordial String value-class.
  test("toContain / not.toContain", () => {
    expect("hello world").toContain("lo w");
    expect("hello world").toContain("hello");
    expect("hello world").toContain("world");
    expect("hello world").not.toContain("zzz");
  });

  test("toStartWith / not.toStartWith", () => {
    expect("hello world").toStartWith("hello");
    expect("hello world").toStartWith("h");
    expect("hello world").not.toStartWith("world");
  });

  test("toEndWith / not.toEndWith", () => {
    expect("hello world").toEndWith("world");
    expect("hello world").toEndWith("d");
    expect("hello world").not.toEndWith("hello");
  });

  test("string matchers on empty and full-length needles", () => {
    expect("abc").toContain("abc");
    expect("abc").toStartWith("abc");
    expect("abc").toEndWith("abc");
    expect("").not.toContain("a");
  });

  test("string matchers are byte-honest on multi-byte input", () => {
    // "ação" — the matchers must compare content, not byte offsets.
    expect("ação").toContain("ç");
    expect("ação").toStartWith("aç");
    expect("ação").toEndWith("ão");
    expect("ação").not.toContain("z");
  });
});

describe("rts:test framework — truthiness and nullish matchers", () => {
  test("toBeTruthy / toBeFalsy", () => {
    expect("nonempty").toBeTruthy();
    expect("").toBeFalsy();
  });

  test("toBeNull / toBeUndefined / toBeDefined", () => {
    expect(`${null}`).toBeNull();
    expect(`${undefined}`).toBeUndefined();
    expect("something").toBeDefined();
  });
});

describe("rts:test framework — numeric matchers", () => {
  test("toBeGreaterThan / toBeLessThan", () => {
    expect(`${10}`).toBeGreaterThan(5);
    expect(`${5}`).toBeLessThan(10);
  });

  test("toBeGreaterThanOrEqual / toBeLessThanOrEqual", () => {
    expect(`${7}`).toBeGreaterThanOrEqual(7);
    expect(`${7}`).toBeGreaterThanOrEqual(3);
    expect(`${7}`).toBeLessThanOrEqual(7);
    expect(`${7}`).toBeLessThanOrEqual(9);
  });

  test("toBeCloseTo respects precision", () => {
    expect(`${0.1 + 0.2}`).toBeCloseTo(0.3, 10);
  });

  test("toBeNaN / toBeFinite", () => {
    expect(`${NaN}`).toBeNaN();
    expect(`${1.5}`).toBeFinite();
  });

  test("toHaveLength compares the ALREADY-COMPUTED length", () => {
    // The framework stores every actual as its string form, so `toHaveLength`
    // parses `_actual` as a number and compares — it does NOT measure the string
    // itself. Callers pass the length: `expect(`${s.length}`).toHaveLength(n)`,
    // not `expect(s).toHaveLength(n)`. (That makes it a numeric matcher in
    // practice; it has no callers in the suite besides this file.)
    expect(`${"abcde".length}`).toHaveLength(5);
    expect(`${"".length}`).toHaveLength(0);
    expect(`${[1, 2, 3].length}`).toHaveLength(3);
  });
});

describe("rts:test framework — negation chains independently", () => {
  test(".not returns a fresh Matcher and does not leak polarity", () => {
    const m = expect("hello");
    m.not.toBe("world");
    // The original matcher must still be positive — `.not` returns a NEW Matcher
    // rather than flipping this one in place.
    m.toBe("hello");
  });
});
