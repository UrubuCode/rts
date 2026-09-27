import { describe, test, expect } from "rts:test";

// `f.call(thisArg, …)` with up to three arguments, `f.apply(thisArg, list)` and
// `String(n)` compile to one entry each where the program leaves `Function` and
// `String` alone. The entry checks the RECEIVER: a plain function is called in
// place, and anything else — an object with its own `call`, a function given an
// own `call`, one whose prototype was replaced — takes the member it actually
// has. What this pins is the language's answer in every one of those shapes.

function add(this: any, a: number, b: number, c: number): number {
  return (this?.base ?? 0) + (a ?? 0) + (b ?? 0) + (c ?? 0);
}
function who(this: any): string { return this === undefined ? "undefined" : this === null ? "null" : String(this.name ?? typeof this); }

describe("call and apply as direct entries", () => {
  test("call passes the receiver and zero to three arguments", () => {
    expect(add.call({ base: 100 })).toBe(100);
    expect(add.call({ base: 100 }, 1)).toBe(101);
    expect(add.call({ base: 100 }, 1, 2)).toBe(103);
    expect(add.call({ base: 100 }, 1, 2, 3)).toBe(106);
    expect(add.call(null, 1, 2, 3)).toBe(6);
    expect(who.call({ name: "o" })).toBe("o");
    expect(who.call(null)).toBe("null");
    let n = 0;
    for (let i = 0; i < 1000; i++) n = add.call(null, n, 1);
    expect(n).toBe(1000);
  });
  test("arguments.length is what was written, not the padding", () => {
    function count(this: any): number { return arguments.length; }
    expect((count as any).call(null)).toBe(0);
    expect((count as any).call(null, 1)).toBe(1);
    expect((count as any).call(null, 1, 2, 3)).toBe(3);
    expect((count as any).call(null, 1, 2, 3, 4, 5)).toBe(5);
    function rest(this: any, ...xs: number[]): number { return xs.length; }
    expect((rest as any).call(null, 1, undefined)).toBe(2);
  });
  test("apply spreads a real array, an array-like, and nothing", () => {
    expect(add.apply({ base: 10 }, [1, 2, 3])).toBe(16);
    expect(add.apply(null, [1])).toBe(1);
    expect((add as any).apply(null)).toBe(0);
    expect((add as any).apply(null, undefined)).toBe(0);
    expect((add as any).apply(null, { length: 2, 0: 5, 1: 6 })).toBe(11);
    function count(this: any): number { return arguments.length; }
    expect((count as any).apply(null, [1, 2, 3, 4, 5, 6])).toBe(6);
    expect((count as any).apply(null, new Array(3))).toBe(3);
    let refused = "";
    try { (add as any).apply(null, 5); } catch (e: any) { refused = e.message; }
    expect(refused.length > 0).toBe(true);
    const m = Math.max.apply(null, [3, 9, 2]);
    expect(m).toBe(9);
  });
  test("a receiver that is not a plain function takes the member it has", () => {
    const o: any = { call(x: number) { return `own:${x}`; }, apply(t: any, xs: number[]) { return `applied:${xs.length}`; } };
    expect(o.call(1)).toBe("own:1");
    expect(o.apply(null, [1, 2])).toBe("applied:2");
    const f: any = function (this: any, x: number) { return x * 2; };
    f.call = (t: any, x: number) => `patched:${x}`;
    expect(f.call(null, 4)).toBe("patched:4");
    expect(f.apply(null, [4])).toBe(8);
    const g: any = function (this: any, x: number) { return x + 1; };
    Object.setPrototypeOf(g, { call: () => "proto", apply: () => "proto-apply" });
    expect(g.call(null, 1)).toBe("proto");
    expect(g.apply(null, [1])).toBe("proto-apply");
    class K { call(x: number) { return x - 1; } }
    const k: any = new K();
    expect(k.call(5)).toBe(4);
    const bound = add.bind({ base: 1000 });
    expect(bound.call({ base: 5 }, 1)).toBe(1001);
    expect(bound.apply({ base: 5 }, [1, 1])).toBe(1002);
  });
  test("a missing member or a non-callable receiver throws the language's TypeError", () => {
    const o: any = { n: 1 };
    let message = "";
    try { o.call(1); } catch (e: any) { message = e.message; }
    expect(message.endsWith("is not a function")).toBe(true);
    let nul = "";
    try { (null as any).apply(null, []); } catch (e: any) { nul = e.message; }
    expect(nul.length > 0).toBe(true);
  });
  test("call and apply reach natives and class methods too", () => {
    expect(Array.prototype.join.call([1, 2], "-")).toBe("1-2");
    expect(Object.prototype.toString.call([])).toBe("[object Array]");
    expect(Object.prototype.hasOwnProperty.call({ a: 1 }, "a")).toBe(true);
    class P { constructor(public v: number) {} get(): number { return this.v; } }
    const p = new P(7);
    expect(P.prototype.get.call(p)).toBe(7);
    expect(P.prototype.get.apply(new P(9), [])).toBe(9);
    const log: string[] = [];
    function order(this: any, a: string, b: string) { log.push(a, b); }
    order.call(null, (log.push("x"), "a"), (log.push("y"), "b"));
    expect(log.join()).toBe("x,y,a,b");
  });
  test("String() over numbers spells like a template, and over others still converts", () => {
    let acc = 0;
    for (let i = 0; i < 200; i++) acc += String(i).length;
    expect(acc).toBe(10 + 90 * 2 + 100 * 3);
    expect(String(0.1 + 0.2)).toBe("0.30000000000000004");
    expect(String(-0)).toBe("0");
    expect(String(1e21)).toBe("1e+21");
    expect(String(NaN)).toBe("NaN");
    expect(String(Symbol("s"))).toBe("Symbol(s)");
    expect(String({ toString: () => "T", valueOf: () => 1 })).toBe("T");
    expect(String(null)).toBe("null");
    expect(String(undefined)).toBe("undefined");
    expect(String([1, [2, 3]])).toBe("1,2,3");
    expect(String(12n)).toBe("12");
    const s = "same";
    expect(String(s) === s).toBe(true);
  });
});
