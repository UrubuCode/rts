import { describe, test, expect } from "rts:test";

// An assignment inside one arm of an expression that branches writes on that path
// only. What the other path holds after the join is what it held before -- a merge,
// which the MIR stage's `?:`, `&&`, `||` and `??` did not make (a silent wrong
// answer on main) and which neither stage made for an optional chain that assigns.

function conditional(c: boolean): string {
  let x = 0;
  const y = c ? (x = 1) : 2;
  return x + ":" + y;
}

function shortCircuits(c: any): string {
  let a = 0, b = 0, d = 0;
  c && (a = 5);
  c || (b = 7);
  const r = c ?? (d = 9);
  return [a, b, d, r].join();
}

function optionalCall(o: any): number {
  let x = 0;
  o?.m((x = 1));
  return x;
}

function optionalIndex(o: any): string {
  let x = 0;
  const r = o?.[(x = 2)];
  return x + ":" + r;
}

function longerChain(o: any): string {
  let x = 0, y = 0;
  const r = o?.p?.q((x = 3), (y = 4));
  return [x, y, r].join();
}

describe("an assignment inside a branch", () => {
  test("a conditional", () => {
    expect(conditional(false)).toBe("0:2");
    expect(conditional(true)).toBe("1:1");
  });
  test("the short-circuiting operators", () => {
    expect(shortCircuits(false)).toBe("0,7,0,false");
    expect(shortCircuits(true)).toBe("5,0,0,true");
    expect(shortCircuits(null)).toBe("0,7,9,9");
  });
  test("an optional chain", () => {
    expect(optionalCall(null)).toBe(0);
    expect(optionalCall({ m() {} })).toBe(1);
    expect(optionalIndex(null)).toBe("0:undefined");
    expect(optionalIndex({ 2: "two" })).toBe("2:two");
    expect(longerChain(null)).toBe("0,0,");
    expect(longerChain({})).toBe("0,0,");
    expect(longerChain({ p: { q: (u: number, v: number) => u + v } })).toBe("3,4,7");
  });
});
