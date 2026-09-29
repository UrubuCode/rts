import { describe, test, expect } from "rts:test";

// `new C()` on a base class whose constructor reads neither its argument record
// nor `new.target` is built without the bookkeeping those two need. What this
// pins is that a program cannot tell: the object inherits from the class, a
// constructor that answers an object wins, a throw leaves the next construction
// intact, and every class the short path must refuse still does what the
// language says.

class Empty {}
class One { a: number; constructor(a: number) { this.a = a; } }
class Init { a = 1; b = "two"; c = [3]; }
class Method { v: number; constructor(v: number) { this.v = v; } twice(): number { return this.v * 2; } }
class Answers { constructor(other?: object) { if (other) return other as any; } }
class Throws { constructor(n: number) { if (n > 1) throw new RangeError("no " + n); } }
class Holds { inner: One; map: Map<string, number>; constructor(n: number) { this.inner = new One(n); this.map = new Map([["k", n]]); } }
class Base { a: number; constructor(a: number) { this.a = a; } }
class Derived extends Base { b: number; constructor(a: number) { super(a); this.b = a + 1; } }
class Target { seen: unknown; constructor() { this.seen = new.target; } }
class Counts { n: number; constructor(..._rest: number[]) { this.n = arguments.length; } }
class Five { s: number; constructor(a: number, b: number, c: number, d: number, e: number) { this.s = a + b + c + d + e; } }

class Deep extends Derived { c: number; constructor(a: number) { super(a); this.c = a + 2; } }
class Implicit extends Base {}
class Wrong extends Base { constructor() { super(1); return 5 as any; } }
class Native extends Map<string, number> { extra = 1; constructor() { super([["k", 2]]); } }

describe("a derived class built directly", () => {
  test("the chain is walked and every level writes its own", () => {
    const d = new Deep(1);
    expect(d.a + d.b + d.c).toBe(6);
    expect(Object.getPrototypeOf(d)).toBe(Deep.prototype);
    expect(d instanceof Base && d instanceof Derived && d instanceof Deep).toBe(true);
    const i = new (Implicit as any)(8);
    expect(i.a).toBe(8);
    expect(Object.getPrototypeOf(i)).toBe(Implicit.prototype);
  });
  test("a primitive answered is refused, and a native parent makes its own object", () => {
    let refused = "";
    try { new Wrong(); } catch (e: any) { refused = e.name; }
    expect(refused).toBe("TypeError");
    expect(new Deep(2).c).toBe(4);
    const n = new Native();
    expect(n.get("k")).toBe(2);
    expect(n.extra).toBe(1);
    expect(n instanceof Native && n instanceof Map).toBe(true);
  });
});

describe("a class built directly", () => {
  test("the object is an instance, with its fields and its methods", () => {
    const e = new Empty();
    expect(e instanceof Empty).toBe(true);
    expect(Object.getPrototypeOf(e)).toBe(Empty.prototype);
    expect(Object.keys(e).length).toBe(0);
    expect(new One(7).a).toBe(7);
    const i = new Init();
    expect(i.a + i.b + i.c[0]).toBe("1two3");
    expect(new Init().c === i.c).toBe(false);
    expect(new Method(21).twice()).toBe(42);
    expect(e.constructor).toBe(Empty);
  });
  test("a constructor that answers an object produces that one", () => {
    const other = { mine: true };
    expect(new Answers(other) as unknown).toBe(other);
    expect(new Answers() instanceof Answers).toBe(true);
  });
  test("a throw leaves the construction after it whole", () => {
    let caught = "";
    try { new Throws(3); } catch (e: any) { caught = e.name + ":" + e.message; }
    expect(caught).toBe("RangeError:no 3");
    expect(new Throws(1) instanceof Throws).toBe(true);
    expect(new Target().seen).toBe(Target);
    expect(new One(2).a).toBe(2);
  });
  test("constructions nest, and natives built inside inherit from their own", () => {
    const h = new Holds(5);
    expect(h.inner instanceof One).toBe(true);
    expect(h.inner.a).toBe(5);
    expect(h.map instanceof Map).toBe(true);
    expect(h.map instanceof Holds).toBe(false);
    expect(h.map.get("k")).toBe(5);
  });
  test("many of them survive collection with what they were given", () => {
    const kept: One[] = [];
    let sum = 0;
    for (let i = 0; i < 200_000; i++) {
      const o = new One(i);
      if (i % 1000 === 0) kept.push(o);
      sum += o.a;
    }
    expect(sum).toBe((199_999 * 200_000) / 2);
    expect(kept.length).toBe(200);
    expect(kept.every((o, at) => o.a === at * 1000 && o instanceof One)).toBe(true);
  });
  test("what the short path refuses still answers what it always did", () => {
    const d = new Derived(1);
    expect(d.a + d.b).toBe(3);
    expect(d instanceof Base && d instanceof Derived).toBe(true);
    expect(new Target().seen).toBe(Target);
    expect(new (Counts as any)(1, 2, 3).n).toBe(3);
    expect(new Five(1, 2, 3, 4, 5).s).toBe(15);
    const viaReflect = Reflect.construct(One, [9], Method);
    expect(viaReflect.a).toBe(9);
    expect(Object.getPrototypeOf(viaReflect)).toBe(Method.prototype);
    let refused = "";
    try { (One as any)(1); } catch (e: any) { refused = e.message; }
    expect(refused.includes("new")).toBe(true);
    function Old(this: any, v: number) { this.v = v; }
    expect(new (Old as any)(4).v).toBe(4);
    const arrow: any = () => 1;
    let notOne = "";
    try { new arrow(); } catch (e: any) { notOne = e.message; }
    expect(notOne.endsWith("is not a constructor")).toBe(true);
  });
});
