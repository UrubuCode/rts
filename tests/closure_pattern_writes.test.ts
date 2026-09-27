import { describe, test, expect } from "rts:test";

// A closure that writes an outer binding through a pattern -- a destructuring
// assignment, or a loop head that assigns an existing name -- makes that binding
// captured exactly as `k = v` would. Missing the leaves of the pattern left the
// binding in a register the closure could not reach: `Unbound("k")`.

function arrayPattern(): number {
  let k = 1;
  const s = () => { [k] = [2]; };
  s();
  return k;
}

function objectPattern(): number {
  var k = 1;
  const s = () => { ({ k } = { k: 4 }); };
  s();
  return k;
}

function forOfHead(): number {
  let k = 1;
  const s = () => { for (k of [5]) {} };
  s();
  return k;
}

function forOfPatternHead(): number {
  let k = 1;
  const s = () => { for ([k] of [[6]]) {} };
  s();
  return k;
}

function forInHead(): string {
  let k = "x";
  const s = () => { for (k in { q: 1 }) {} };
  s();
  return k;
}

function localFunctionReplaced(): number {
  function k(): number { return 1; }
  const s = () => { [k] = [() => 2] as any; };
  const first = k();
  s();
  return first * 10 + k();
}

describe("closure pattern writes", () => {
  test("a destructuring assignment", () => {
    expect(arrayPattern()).toBe(2);
    expect(objectPattern()).toBe(4);
  });
  test("an assigned loop head", () => {
    expect(forOfHead()).toBe(5);
    expect(forOfPatternHead()).toBe(6);
    expect(forInHead()).toBe("q");
  });
  test("a local function replaced through a pattern", () => {
    expect(localFunctionReplaced()).toBe(12);
  });
});
