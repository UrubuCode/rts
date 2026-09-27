import { describe, test, expect } from "rts:test";

// `...rest` in an object pattern collects the own enumerable properties the pattern
// did not name, into a fresh object, and never reads the ones it did name.

function basic(): string {
  const { a, ...rest } = { a: 1, b: 2, c: 3 };
  return a + "|" + JSON.stringify(rest);
}

function computedAndDefault(): string {
  const k = "b";
  const { [k]: bv, z = 9, ...rest } = { a: 1, b: 2, c: 3 } as any;
  return bv + "|" + z + "|" + JSON.stringify(rest);
}

function namedGetterNotRun(): string {
  const log: string[] = [];
  const src = {
    get a() { log.push("a"); return 1; },
    get b() { log.push("b"); return 2; },
  };
  const { a, ...rest } = src;
  return a + "|" + JSON.stringify(rest) + "|" + log.join();
}

function freshAndNotAliased(): string {
  const src = { a: 1, b: { deep: true } };
  const { ...copy } = src;
  copy.a = 5;
  return src.a + "|" + (copy.b === src.b) + "|" + (copy !== (src as any));
}

function asAssignment(): string {
  let a: any, rest: any;
  ({ a, ...rest } = { a: "x", b: "y" });
  return a + "|" + JSON.stringify(rest);
}

function nested(): string {
  const { outer: { keep, ...inner }, ...others } = { outer: { keep: 1, drop: 2 }, more: 3 } as any;
  return keep + "|" + JSON.stringify(inner) + "|" + JSON.stringify(others);
}

describe("an object pattern's rest", () => {
  test("the unnamed properties", () => expect(basic()).toBe('1|{"b":2,"c":3}'));
  test("a computed key and a default", () => expect(computedAndDefault()).toBe('2|9|{"a":1,"c":3}'));
  test("a named getter runs once, an unnamed one once", () =>
    expect(namedGetterNotRun()).toBe('1|{"b":2}|a,b'));
  test("a fresh object, shallow", () => expect(freshAndNotAliased()).toBe("1|true|true"));
  test("in an assignment", () => expect(asAssignment()).toBe('x|{"b":"y"}'));
  test("nested", () => expect(nested()).toBe('1|{"drop":2}|{"more":3}'));
});
