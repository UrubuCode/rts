import { describe, test, expect } from "rts:test";

// A method with `const` locals, or one that calls another method of its
// class, is rewritten into one expression at the site where the instance is
// never seen. What this pins is the ANSWER staying the one the program wrote:
// a local read across a write of the field it read, a call chain, a
// parameter used twice, an inner call inside a write.

class V {
  x: number;
  y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
  len2() { return this.x * this.x + this.y * this.y; }
  len() { return Math.sqrt(this.len2()); }
  scaled(k: number) { const s = k * 2; return this.x * s + this.y * s; }
  norm() { const d = this.len(); return this.x / d + this.y / d; }
  bump() { this.x = this.x + 1; return this.len2(); }
  twice(k: number) { return this.scaled(k) + this.scaled(k + 1); }
  chained() { const a = this.x + 1; const b = a * 2; return b + a; }
  bad() { const a = this.x; this.x = 5; return a + this.x; }
  rec(n: number): number { return n > 0 ? this.rec(n - 1) + 1 : 0; }
  writesThenCalls(k: number) { this.y = this.scaled(k); return this.y; }
}

describe("a method with locals or inner calls on an unseen instance", () => {
  test("answers what the written program answers", () => {
    const v = new V(3, 4);
    expect(v.len()).toBe(5);
    expect(v.scaled(2)).toBe(28);
    expect(v.norm()).toBeCloseTo(1.4, 6);
    expect(v.bump()).toBe(32);
    expect(v.twice(1)).toBe((4 * 2 + 4 * 2) + (4 * 4 + 4 * 4));
    expect(v.chained()).toBe(15);
  });

  test("a local read before a write of its field keeps the old value", () => {
    const v = new V(1, 2);
    expect(v.bad()).toBe(6);
    expect(v.x).toBe(5);
  });

  test("a recursion and a write fed by an inner call", () => {
    const v = new V(1, 2);
    expect(v.rec(3)).toBe(3);
    expect(v.writesThenCalls(1)).toBe(6);
    expect(v.y).toBe(6);
  });

  test("the same in a loop that accumulates", () => {
    let a = 0;
    for (let i = 0; i < 3; i++) {
      const v = new V(i, 3);
      a += v.len() + v.scaled(2) + v.norm() + v.bump() + v.twice(1);
    }
    expect(a.toFixed(4)).toBe("192.4195");
  });
});
