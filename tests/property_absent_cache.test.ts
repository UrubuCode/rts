import { describe, test, expect } from "rts:test";

// A read of a property the object does NOT have is cached like one it has:
// the site remembers "absent, answer undefined" and stops asking the runtime.
// What makes that legal is everything below — each case is a way the answer
// can stop being `undefined` without the receiver's own layout changing, and
// each one must be seen by a site that had already warmed on the absence.

function readMany(o: any, n: number): number {
  let c = 0;
  for (let i = 0; i < n; i++) if (o.zz === undefined) c++;
  return c;
}
function readLast(o: any, n: number): any {
  let v: any = 0;
  for (let i = 0; i < n; i++) v = o.zz;
  return v;
}

describe("an absent property is cached, and every way it can appear is seen", () => {
  test("warm absence answers undefined", () => {
    const o: any = { a: 1 };
    expect(readMany(o, 1000)).toBe(1000);
    expect(o.zz ?? "dflt").toBe("dflt");
  });

  test("adding it to the receiver is seen", () => {
    const o: any = { a: 1 };
    expect(readMany(o, 500)).toBe(500);
    o.zz = 7;
    expect(readLast(o, 3)).toBe(7);
  });

  test("adding it to a prototype in the chain is seen", () => {
    class A {}
    const a: any = new A();
    expect(readMany(a, 500)).toBe(500);
    (A.prototype as any).zz = 11;
    expect(readLast(a, 3)).toBe(11);
    expect(readMany(a, 3)).toBe(0);
  });

  test("adding it to Object.prototype is seen, and removed again", () => {
    const o: any = { a: 1 };
    expect(readMany(o, 500)).toBe(500);
    (Object.prototype as any).zz = 13;
    expect(readLast(o, 3)).toBe(13);
    delete (Object.prototype as any).zz;
    expect(readMany(o, 3)).toBe(3);
  });

  test("a getter defined on a prototype is seen", () => {
    class B {}
    const b: any = new B();
    expect(readMany(b, 500)).toBe(500);
    Object.defineProperty(B.prototype, "zz", { get() { return 17; }, configurable: true });
    expect(readLast(b, 3)).toBe(17);
  });

  test("relinking a prototype higher up is seen", () => {
    class C {}
    const c: any = new C();
    expect(readMany(c, 500)).toBe(500);
    Object.setPrototypeOf(C.prototype, { zz: 19 });
    expect(readLast(c, 3)).toBe(19);
  });

  test("relinking the receiver itself is seen", () => {
    const o: any = Object.create(null);
    expect(readMany(o, 500)).toBe(500);
    Object.setPrototypeOf(o, { zz: 23 });
    expect(readLast(o, 3)).toBe(23);
  });

  test("a proxy in the chain answers every time", () => {
    const p = new Proxy({}, { get: (_t, k) => (k === "zz" ? 29 : undefined) });
    const o: any = Object.create(p);
    expect(readLast(o, 500)).toBe(29);
  });

  test("a private name that is nowhere still throws", () => {
    class D {
      #v = 1;
      static read(o: any): any { return o.#v; }
    }
    let threw = 0;
    for (let i = 0; i < 50; i++) {
      try { D.read({}); } catch (e) { if (e instanceof TypeError) threw++; }
    }
    expect(threw).toBe(50);
  });

  test("two receivers of different layouts at one site", () => {
    const a: any = { a: 1 };
    const b: any = { b: 2, zz: 31 };
    let sum = 0;
    for (let i = 0; i < 400; i++) {
      const o = i & 1 ? a : b;
      sum += o.zz === undefined ? 1 : o.zz;
    }
    expect(sum).toBe(200 * 1 + 200 * 31);
  });

  test("the `in` operator agrees after a warm absent read", () => {
    const o: any = { a: 1 };
    expect(readMany(o, 200)).toBe(200);
    expect("zz" in o).toBe(false);
    (Object.prototype as any).zz = 1;
    expect("zz" in o).toBe(true);
    expect(o.zz).toBe(1);
    delete (Object.prototype as any).zz;
  });
});
