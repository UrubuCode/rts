import { describe, test, expect } from "rts:test";

// A class whose constructor only fills fields, constructed into a local that
// nothing else sees, is compiled as the fields themselves, and a method that is
// one expression over them is compiled as that expression. What this pins is
// that a program cannot tell: every value is what the constructor would have
// written, arguments run once and in the order written, and an instance that
// IS seen — returned, stored, passed, asked for its class — is the instance.
//
// `Vec`, `Init`, `Mixed`, `Swapped`, `Param` and `Late` are never read as
// values here, which is what lets them have a layout. `Point` is, on purpose.

class Vec {
  x: number; y: number;
  constructor(x: number, y: number) { this.x = x; this.y = y; }
  norm(): number { return this.x * this.x + this.y * this.y; }
  plus(d: number): number { return this.x + this.y + d; }
  pick(first: boolean): number { return first ? this.x : this.y; }
  scaled(k: number, bias: number): number { return (this.x + this.y) * k + bias; }
  describe(): string { return "v" + this.x; }
  logged(into: string[]): number { into.push("seen"); return this.x; }
}
class Init { a = 1; b = "two"; c = 2 + 3; }
class Mixed { tag = "m"; v: number; w = 0; constructor(v: number) { this.v = v; this.w = 9; } }
class Swapped { first: number; second: number; constructor(a: number, b: number) { this.second = b; this.first = a; } }
class Param { constructor(public p: number, public q: string) {} }
class Point { x: number; y: number; constructor(x: number, y: number) { this.x = x; this.y = y; } norm(): number { return this.x * this.x + this.y * this.y; } }
class Guarded { _v = 0; get v(): number { return this._v + 1; } set v(n: number) { this._v = n * 2; } constructor(n: number) { this.v = n; } }
class Computes { total: number; constructor(a: number, b: number) { this.total = a + b; } }
class Calls { made: number; constructor() { this.made = Calls.count++; } static count = 0; }

describe("an instance nothing else sees", () => {
  test("its fields are what the constructor writes", () => {
    let sum = 0;
    for (let i = 0; i < 1000; i++) {
      const p = new Vec(i, 2);
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
  test("a field may be written and read back", () => {
    const p = new Vec(1, 2);
    p.x = p.x + 10;
    p.y += 1;
    expect(p.x * 100 + p.y).toBe(1103);
    let acc = 0;
    for (let i = 0; i < 100; i++) {
      const c = new Vec(i, i);
      c.x = c.x * 2;
      acc += c.x + c.y;
    }
    expect(acc).toBe(3 * 4950);
  });
  test("a method answers from the fields as they are when it is called", () => {
    let sum = 0;
    for (let i = 0; i < 1000; i++) {
      const v = new Vec(i, 1);
      sum += v.norm();
    }
    expect(sum).toBe(332833500 + 1000);
    const v = new Vec(3, 4);
    expect(v.norm()).toBe(25);
    v.x = 6;
    expect(v.norm()).toBe(52);
    expect(v.plus(10)).toBe(20);
    const ten = 10;
    expect(v.plus(ten) + v.scaled(2, 1)).toBe(20 + 21);
    expect(v.pick(true) * 10 + v.pick(false)).toBe(64);
    expect((v as any).plus()).toBe(NaN);
    expect(v.describe()).toBe("v6");
  });
  test("a method that does more than compute is called, and sees the instance", () => {
    const log: string[] = [];
    const v = new Vec(7, 8);
    expect(v.logged(log)).toBe(7);
    expect(log.join()).toBe("seen");
    const w = new Vec(1, 2);
    const arg = () => { log.push("arg"); return 5; };
    expect(w.plus(arg())).toBe(8);
    expect(log.join()).toBe("seen,arg");
  });
  test("arguments run once each, in the order written", () => {
    const log: string[] = [];
    const a = () => { log.push("a"); return 1; };
    const b = () => { log.push("b"); return 2; };
    const p = new Vec(a(), b());
    expect(p.x + p.y).toBe(3);
    const s = new Swapped(a(), b());
    expect(s.first * 10 + s.second).toBe(12);
    expect(log.join()).toBe("a,b,a,b");
    const few = new (Vec as any)(7);
    expect(few.x).toBe(7);
    expect(few.y).toBe(undefined);
  });
  test("a class used before it holds its class still refuses", () => {
    let name = "";
    try {
      const early = new Late(1);
      name = "built " + early.v;
    } catch (e: any) { name = e.name; }
    expect(name.endsWith("Error")).toBe(true);
    class Late { v: number; constructor(v: number) { this.v = v; } }
    const now = new Late(2);
    expect(now.v).toBe(2);
  });
});

class Shape { kind = "shape"; id: number; constructor(id: number) { this.id = id; } label(): string { return this.kind + this.id; } twice(): number { return this.id * 2; } }
class Rect extends Shape { w: number; h: number; constructor(id: number, w: number, h: number) { super(id); this.w = w; this.h = h; this.kind = "rect"; } area(): number { return this.w * this.h; } twice(): number { return this.id * 20; } }
class Square extends Rect { side: number; constructor(id: number, side: number) { super(id, side, side); this.side = side; } }
class Tagged extends Shape { tag = "t"; }
class Fixed extends Shape { constructor() { super(7); } }
class Seen extends Point { z: number; constructor(z: number) { super(1, 2); this.z = z; } }
class Before extends Shape { n: number; constructor(n: number) { super(n); this.n = this.id + 1; } }

describe("a class that extends one with a layout", () => {
  test("has the parent's fields, then its own", () => {
    let sum = 0;
    for (let i = 0; i < 1000; i++) {
      const r = new Rect(i, 2, 3);
      sum += r.id + r.w * r.h;
    }
    expect(sum).toBe(499500 + 6000);
    const r = new Rect(5, 2, 3);
    expect(r.kind + r.id + r.w + r.h).toBe("rect523");
    const s = new Square(9, 4);
    expect(s.kind + s.id + s.w + s.h + s.side).toBe("rect9444");
    const t = new (Tagged as any)(3);
    expect(t.kind + t.id + t.tag).toBe("shape3t");
    const f = new Fixed();
    expect(f.id).toBe(7);
  });
  test("reads its parent's methods, and its own over them", () => {
    const r = new Rect(5, 2, 3);
    expect(r.area()).toBe(6);
    expect(r.label()).toBe("rect5");
    expect(r.twice()).toBe(100);
    const s = new Square(2, 4);
    expect(s.area() + s.twice()).toBe(16 + 40);
    const plain = new Shape(4);
    expect(plain.twice() + plain.label()).toBe("8shape4");
  });
  test("arguments run once, in order, through every level", () => {
    const log: string[] = [];
    const a = () => { log.push("a"); return 1; };
    const b = () => { log.push("b"); return 2; };
    const c = () => { log.push("c"); return 3; };
    const r = new Rect(a(), b(), c());
    expect(r.id * 100 + r.w * 10 + r.h).toBe(123);
    expect(log.join()).toBe("a,b,c");
    const s = new Square(a(), b());
    expect(s.id * 1000 + s.w * 100 + s.h * 10 + s.side).toBe(1222);
    expect(log.join()).toBe("a,b,c,a,b");
  });
  test("a parent that is seen, or a constructor that reads, is constructed", () => {
    const seen = new Seen(3);
    expect(seen.x + seen.y + seen.z).toBe(6);
    expect(seen.norm()).toBe(5);
    const before = new Before(4);
    expect(before.n).toBe(5);
    const kept = [new Rect(1, 2, 3)];
    expect(kept[0].area() + kept[0].label()).toBe("6rect1");
  });
});

describe("an instance that is seen, of a class with a layout", () => {
  test("is an object of that class, with its fields in order and its methods", () => {
    const held: Vec[] = [];
    for (let i = 0; i < 4; i++) held.push(new Vec(i, i + 1));
    expect(held.map((v) => v.norm()).join()).toBe("1,5,13,25");
    expect(Object.keys(held[0]).join()).toBe("x,y");
    expect(JSON.stringify(held[2])).toBe('{"x":2,"y":3}');
    const first = Object.getPrototypeOf(held[0]);
    expect(held.every((v) => Object.getPrototypeOf(v) === first)).toBe(true);
    expect(typeof first.norm).toBe("function");
    expect(Object.getPrototypeOf(first)).toBe(Object.prototype);
    const log: string[] = [];
    expect(held[3].logged(log)).toBe(3);
    expect(held[1].describe() + held[1].plus(1)).toBe("v14");
    expect(Object.prototype.hasOwnProperty.call(held[0], "x")).toBe(true);
    expect(Object.prototype.hasOwnProperty.call(held[0], "norm")).toBe(false);
  });
  test("is the same kind of object whichever way it was made", () => {
    const make = (n: number) => new Vec(n, n);
    const direct = make(1);
    const local = new Vec(2, 2);
    const kept = [local];
    const spread = new (Vec as any)(...[3, 3]);
    expect(Object.getPrototypeOf(direct)).toBe(Object.getPrototypeOf(kept[0]));
    expect(Object.getPrototypeOf(spread)).toBe(Object.getPrototypeOf(direct));
    let sum = 0;
    const mixed = [direct, kept[0], spread];
    for (let i = 0; i < 300; i++) sum += mixed[i % 3].x + mixed[i % 3].norm();
    expect(sum).toBe(100 * (1 + 2 + 3) + 100 * (2 + 8 + 18));
  });
  test("a derived one inherits through every level, and is built with its arguments in order", () => {
    const log: string[] = [];
    const a = () => { log.push("a"); return 1; };
    const b = () => { log.push("b"); return 2; };
    const c = () => { log.push("c"); return 3; };
    const kept = [new Rect(a(), b(), c()), new Square(a(), b())];
    expect(log.join()).toBe("a,b,c,a,b");
    expect(Object.keys(kept[0]).join()).toBe("kind,id,w,h");
    expect(Object.keys(kept[1]).join()).toBe("kind,id,w,h,side");
    expect(kept[0].area() + kept[0].label() + kept[0].twice()).toBe("6rect120");
    expect(kept[1].area() + kept[1].label() + kept[1].twice()).toBe("4rect120");
    const rect = Object.getPrototypeOf(kept[0]);
    const square = Object.getPrototypeOf(kept[1]);
    expect(Object.getPrototypeOf(square)).toBe(rect);
    expect(typeof Object.getPrototypeOf(rect).label).toBe("function");
    const many: Rect[] = [];
    for (let i = 0; i < 50_000; i++) { const r = new Rect(i, 1, 2); if (i % 5000 === 0) many.push(r); }
    expect(many.map((r) => r.id).join()).toBe("0,5000,10000,15000,20000,25000,30000,35000,40000,45000");
  });
});

class Asked { v: number; constructor(v: number) { this.v = v; } twice(): number { return this.v * 2; } }
class Other { v = 0; }

describe("a class that is asked about", () => {
  test("keeps its layout, and answers for every instance however it was made", () => {
    let sum = 0;
    for (let i = 0; i < 1000; i++) { const a = new Asked(i); sum += a.twice(); }
    expect(sum).toBe(999000);
    const kept = [new Asked(1), new Asked(2)];
    expect(kept.every((a) => a instanceof Asked)).toBe(true);
    expect(kept[0] instanceof Other).toBe(false);
    const local = new Asked(3);
    expect(local instanceof Asked).toBe(true);
    expect(local.twice()).toBe(6);
    expect(({ v: 1 }) instanceof Asked).toBe(false);
    expect((new Other() as any) instanceof Asked).toBe(false);
  });
});

describe("a class declared inside a function", () => {
  test("has a layout on every call, and a prototype of its own on each", () => {
    const make = (k: number) => {
      class Inside { v: number; constructor(v: number) { this.v = v; } scaled(): number { return this.v * 3; } }
      let sum = 0;
      for (let i = 0; i < 100; i++) { const o = new Inside(i + k); sum += o.scaled(); }
      return { sum, one: new Inside(k) };
    };
    const first = make(0), second = make(1);
    expect(first.sum).toBe(3 * 4950);
    expect(second.sum).toBe(3 * 5050);
    expect(first.one.scaled() + second.one.scaled()).toBe(3);
    expect(Object.getPrototypeOf(first.one) === Object.getPrototypeOf(second.one)).toBe(false);
    expect(Object.keys(second.one).join()).toBe("v");
  });
  test("and one that extends another declared beside it", () => {
    const run = () => {
      class Low { a: number; constructor(a: number) { this.a = a; } low(): number { return this.a; } }
      class High extends Low { b: number; constructor(a: number, b: number) { super(a); this.b = b; } high(): number { return this.a + this.b; } }
      const local = new High(1, 2);
      const kept = [new High(3, 4)];
      return local.high() * 100 + local.low() * 10 + kept[0].high() + kept[0].low();
    };
    expect(run()).toBe(300 + 10 + 7 + 3);
  });
  test("captures nothing it should not: a constructor reading outside is constructed", () => {
    const outside = 5;
    class Reads { v: number; constructor() { this.v = outside; } }
    const r = new Reads();
    expect(r.v).toBe(5);
  });
});

describe("classes of one name in different functions", () => {
  test("are each their own class", () => {
    const first = () => { class Same { v = 1; one(): number { return this.v; } } const o = new Same(); return [o.one(), new Same()] as const; };
    const second = () => { class Same { v: string; w = 2; constructor(v: string) { this.v = v; } both(): string { return this.v + this.w; } } const o = new Same("s"); return [o.both(), new Same("k")] as const; };
    const [a, keptA] = first();
    const [b, keptB] = second();
    expect(a).toBe(1);
    expect(b).toBe("s2");
    expect(Object.keys(keptA).join()).toBe("v");
    expect(Object.keys(keptB).join()).toBe("v,w");
    expect((keptA as any).one() + (keptB as any).both()).toBe("1k2");
    expect(Object.getPrototypeOf(keptA) === Object.getPrototypeOf(keptB)).toBe(false);
  });
  test("and a name shadowed in an inner block means what it means there", () => {
    class Outer { v: number; constructor(v: number) { this.v = v; } }
    let inner = 0;
    { class Outer { w = 9; } const o = new Outer(); inner = o.w; }
    const o = new Outer(4);
    expect(inner * 10 + o.v).toBe(94);
  });
});

class Body {
  x: number; y: number; n = 0; tag = "b";
  constructor(x: number, y: number) { this.x = x; this.y = y; }
  length(): number { return Math.sqrt(this.x * this.x + this.y * this.y); }
  scale(k: number): void { this.x = this.x * k; this.y *= k; }
  bump(): number { this.n += 1; return this.n; }
  tick(): void { this.n++; }
  rename(to: string): string { this.tag = to; return this.tag + this.n; }
  far(limit: number): boolean { return Math.max(Math.abs(this.x), Math.abs(this.y)) > limit; }
}

describe("a method that writes its own fields", () => {
  test("writes them, in order, and answers from what it wrote", () => {
    let total = 0;
    for (let i = 0; i < 1000; i++) {
      const b = new Body(3, 4);
      b.scale(2);
      total += b.length() + b.bump() + b.bump();
    }
    expect(total).toBe(1000 * (10 + 1 + 2));
    const b = new Body(1, 2);
    b.tick(); b.tick();
    expect(b.bump()).toBe(3);
    expect(b.n).toBe(3);
    expect(b.rename("z")).toBe("z3");
    expect(b.tag).toBe("z");
    b.scale(-3);
    expect(b.x * 10 + b.y).toBe(-36);
    expect(b.far(5)).toBe(true);
    expect(b.far(6)).toBe(false);
    const used = b.scale(1);
    expect(used).toBe(undefined);
  });
  test("and an instance written to across a loop, or seen, is still the instance", () => {
    const across = new Body(1, 1);
    for (let i = 0; i < 10; i++) across.tick();
    expect(across.n).toBe(10);
    const kept = [new Body(3, 4)];
    kept[0].scale(2);
    expect(kept[0].length() + kept[0].bump()).toBe(11);
    const text = new Body(1, 2);
    (text as any).n = "a";
    text.tick();
    expect(Number.isNaN(text.n)).toBe(true);
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
    const keyed = new Point(5, 6);
    expect(Object.keys(keyed).join()).toBe("x,y");
    const shown = new Init();
    expect(JSON.stringify(shown)).toBe('{"a":1,"b":"two","c":5}');
    const closed = new Vec(8, 9);
    const read = () => closed.x + closed.norm();
    expect(read()).toBe(8 + 145);
    const ctor = new Point(1, 2);
    expect(ctor.constructor).toBe(Point);
    const stored = { inner: new Vec(1, 2) };
    expect(stored.inner.norm()).toBe(5);
  });
  test("a method installed from outside is the one that runs", () => {
    (Point.prototype as any).norm = function (this: Point) { return -1; };
    const p = new Point(3, 4);
    expect(p.norm()).toBe(-1);
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
