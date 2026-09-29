import { describe, test, expect } from "rts:test";

// A class whose constructor only fills fields, constructed into a local that
// nothing else sees, is compiled as the fields themselves. What this pins is
// that a program cannot tell: every value is what the constructor would have
// written, arguments run once and in the order written, and an instance that
// IS seen — returned, stored, passed, asked for a method or its class — is the
// instance.

class Point { x: number; y: number; constructor(x: number, y: number) { this.x = x; this.y = y; } norm(): number { return this.x * this.x + this.y * this.y; } }
class Init { a = 1; b = "two"; c = 2 + 3; }
class Mixed { tag = "m"; v: number; w = 0; constructor(v: number) { this.v = v; this.w = 9; } }
class Swapped { first: number; second: number; constructor(a: number, b: number) { this.second = b; this.first = a; } }
class Param { constructor(public p: number, public q: string) {} }
class Guarded { _v = 0; get v(): number { return this._v + 1; } set v(n: number) { this._v = n * 2; } constructor(n: number) { this.v = n; } }
class Computes { total: number; constructor(a: number, b: number) { this.total = a + b; } }
class Calls { made: number; constructor() { this.made = Calls.count++; } static count = 0; }

describe("an instance nothing else sees", () => {
  test("its fields are what the constructor writes", () => {
    let sum = 0;
    for (let i = 0; i < 1000; i++) {
      const p = new Point(i, 2);
      sum += p.x + p.y;
    }
    expect(sum).toBe(499500 + 2000);
    const i = new Init();
    expect(i.a + i.b + i.c).toBe("1two5");
    const m = new Mixed(4);
    expect(m.tag + m.v + m.w).toBe("m49");
    const s = new Swapped(1, 2);
    expect(s.first * 10 + s.second).toBe(12);
    const q = new Param(3, "z");
    expect(q.q + q.p).toBe("z3");
  });
  test("a field may be written and read back, and added to", () => {
    const p = new Point(1, 2);
    p.x = p.x + 10;
    p.y += 1;
    expect(p.x * 100 + p.y).toBe(1103);
    let acc = 0;
    for (let i = 0; i < 100; i++) {
      const c = new Point(i, i);
      c.x = c.x * 2;
      acc += c.x + c.y;
    }
    expect(acc).toBe(3 * 4950);
  });
  test("arguments run once each, in the order written", () => {
    const log: string[] = [];
    const a = () => { log.push("a"); return 1; };
    const b = () => { log.push("b"); return 2; };
    const p = new Point(a(), b());
    expect(p.x + p.y).toBe(3);
    const s = new Swapped(a(), b());
    expect(s.first * 10 + s.second).toBe(12);
    expect(log.join()).toBe("a,b,a,b");
    const few = new (Point as any)(7);
    expect(few.x).toBe(7);
    expect(few.y).toBe(undefined);
    const many = new (Point as any)(1, 2, a());
    expect(many.x + many.y).toBe(3);
    expect(log.length).toBe(5);
  });
  test("a class used before it is initialised still refuses", () => {
    let name = "";
    try {
      const early = new Late(1);
      name = "built " + early.v;
    } catch (e: any) { name = e.name; }
    expect(name).toBe("ReferenceError");
    class Late { v: number; constructor(v: number) { this.v = v; } }
    const now = new Late(2);
    expect(now.v).toBe(2);
  });
});

describe("an instance that is seen is the instance", () => {
  test("returned, stored, passed, or asked what it is", () => {
    const make = (n: number) => { const p = new Point(n, n); return p; };
    expect(make(2) instanceof Point).toBe(true);
    expect(make(2).norm()).toBe(8);
    const held: Point[] = [];
    for (let i = 0; i < 3; i++) { const p = new Point(i, 0); held.push(p); }
    expect(held.map((p) => p.x).join()).toBe("0,1,2");
    expect(held.every((p) => Object.getPrototypeOf(p) === Point.prototype)).toBe(true);
    const asked = new Point(3, 4);
    expect(asked.norm()).toBe(25);
    const kind = new Point(1, 1);
    expect(kind instanceof Point).toBe(true);
    const keyed = new Point(5, 6);
    expect(Object.keys(keyed).join()).toBe("x,y");
    const shown = new Init();
    expect(JSON.stringify(shown)).toBe('{"a":1,"b":"two","c":5}');
    const closed = new Point(8, 9);
    const read = () => closed.x + closed.y;
    expect(read()).toBe(17);
    const ctor = new Point(1, 2);
    expect(ctor.constructor).toBe(Point);
  });
  test("a constructor that does more than fill fields runs", () => {
    const g = new Guarded(5);
    expect(g._v).toBe(10);
    expect(g.v).toBe(11);
    const c = new Computes(2, 3);
    expect(c.total).toBe(5);
    const first = new Calls();
    const second = new Calls();
    expect(first.made * 10 + second.made).toBe(1);
    expect(Calls.count).toBe(2);
  });
});
