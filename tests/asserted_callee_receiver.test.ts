import { describe, test, expect } from "rts:test";

// A type assertion around a callee is erased before the call is decided, so
// `(o.m as any)(x)` calls `o.m` ON `o` exactly as `o.m(x)` does. Read as a
// value, the callee lost its receiver and a method reading `this.length` read
// `undefined`'s — an uncaught TypeError, not a wrong answer, from a cast that
// TypeScript erases before the program exists.

class Counter {
  n = 0;
  bump(by: number): number { this.n += by; return this.n; }
}

describe("a type assertion around a callee keeps the receiver", () => {
  test("a built-in method through a cast", () => {
    const a = [0, 1, 2, 3, 4];
    const r = (a.splice as any)("2", "1");
    expect(r.length).toBe(1);
    expect(r[0]).toBe(2);
    expect(a.join()).toBe("0,1,3,4");
    expect((a.indexOf as any)(3)).toBe(2);
  });
  test("a user method through a cast, inside an arrow", () => {
    const c = new Counter();
    const run = () => (c.bump as any)(2);
    expect(run()).toBe(2);
    expect(run()).toBe(4);
    expect((c.bump as any)(1)).toBe(5);
  });
  // `(o?.m)()` loses its receiver in both emitters, cast or not, and Node keeps
  // it; that is a gap of its own and not this fixture's.
  test("a computed member and a string method through a cast", () => {
    const o: any = { k: 3, m() { return this.k; } };
    expect((o["m"] as any)()).toBe(3);
    const s = "abc";
    expect((s.toUpperCase as any)()).toBe("ABC");
  });
});
