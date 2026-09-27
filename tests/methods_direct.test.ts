import { describe, test, expect } from "rts:test";

// `m.get(k)`, `m.has(k)`, `m.set(k, v)`, `s.has(v)`, `s.add(v)` and `a.push(v)`
// are compiled as one entry each where the program leaves `Map`, `Set` and
// `Array` alone. The entry checks the receiver's brand: a real instance answers
// from its table, ANYTHING ELSE takes the method it actually has. What this
// pins is that the fallback is the language's call — a plain object with a
// `get`, a class with its own `has`, a frozen array, a missing method — and
// that the real instances answer what they always did.

class Bag {
  items: number[] = [];
  has(x: number): boolean { return this.items.includes(x); }
  add(x: number): this { this.items.push(x); return this; }
  get(k: number): string { return `bag:${k}`; }
  set(k: number, v: number): string { return `set:${k}=${v}`; }
  push(v: number): string { return `pushed:${v}`; }
}

describe("collection methods as direct entries", () => {
  test("a real Map and Set answer from their tables", () => {
    const m = new Map<any, any>();
    expect(m.set("a", 1)).toBe(m);
    m.set(NaN, "nan").set(-0, "zero");
    expect(m.get("a")).toBe(1);
    expect(m.get("zz")).toBe(undefined);
    expect(m.has("a")).toBe(true);
    expect(m.has("zz")).toBe(false);
    expect(m.get(NaN)).toBe("nan");
    expect(m.get(0)).toBe("zero");
    const s = new Set<any>();
    expect(s.add(1)).toBe(s);
    s.add(1).add(2).add(-0);
    expect(s.has(1)).toBe(true);
    expect(s.has(3)).toBe(false);
    expect(s.has(0)).toBe(true);
    expect(s.size).toBe(3);
    let hits = 0;
    for (let i = 0; i < 1000; i++) { m.set(i, i * 2); if (m.has(i) && m.get(i) === i * 2) hits++; }
    expect(hits).toBe(1000);
  });
  test("a plain object or a class with the same member names is called, not branded", () => {
    const b: any = new Bag();
    expect(b.has(1)).toBe(false);
    expect(b.add(1)).toBe(b);
    expect(b.has(1)).toBe(true);
    expect(b.get(7)).toBe("bag:7");
    expect(b.set(1, 2)).toBe("set:1=2");
    expect(b.push(9)).toBe("pushed:9");
    const o: any = { get(k: any) { return k + 1; }, has(k: any) { return k > 0; }, push(v: any) { return -v; } };
    expect(o.get(1)).toBe(2);
    expect(o.has(1)).toBe(true);
    expect(o.push(4)).toBe(-4);
  });
  test("a receiver without the method throws the language's TypeError", () => {
    const o: any = { n: 1 };
    let message = "";
    try { o.get(1); } catch (e: any) { message = e.message; }
    expect(message.endsWith("is not a function")).toBe(true);
    let onUndefined = "";
    try { (undefined as any).has(1); } catch (e: any) { onUndefined = e.message; }
    expect(onUndefined.length > 0).toBe(true);
    let calls = 0;
    const p: any = { get: 5 };
    try { p.get(calls++); } catch (e: any) { calls += 100; }
    expect(calls).toBe(101);
  });
  test("push appends in place, answers the length, and defers to the member otherwise", () => {
    const a: number[] = [];
    expect(a.push(1)).toBe(1);
    expect(a.push(2)).toBe(2);
    expect(a.join()).toBe("1,2");
    let n = 0;
    for (let i = 0; i < 500; i++) n = a.push(i);
    expect(n).toBe(502);
    expect(a.length).toBe(502);
    const frozen = Object.freeze([1]);
    let refused = "";
    try { (frozen as any).push(2); } catch (e: any) { refused = e.message; }
    expect(refused.length > 0).toBe(true);
    expect(frozen.length).toBe(1);
    const u: any[] = [];
    u.push(undefined);
    expect(u.length).toBe(1);
  });
  test("arguments are evaluated once and in order", () => {
    const log: string[] = [];
    const m = new Map<string, number>();
    const k = () => { log.push("k"); return "key"; };
    const v = () => { log.push("v"); return 7; };
    m.set(k(), v());
    expect(m.get(k())).toBe(7);
    expect(log.join()).toBe("k,v,k");
  });
});
