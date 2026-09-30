import { describe, test, expect } from "rts:test";

// `f(...xs)`, `f.apply(t, [a, b])` and a bound function each reached the callee
// through an array built per call. Each now hands its arguments over without
// one where the convention carries them. What this pins is that nothing a
// program can see changed: the count, the order, the receiver, `arguments`, a
// receiver with its own `apply`, more arguments than the slots, spreading
// something that is not an array, and constructing through a bound function.

function f3(a: number, b: number, c: number): number { return a * 100 + b * 10 + c; }
function who(this: any, ...xs: number[]): string { return String(this?.tag) + ":" + xs.join("+") + ":" + arguments.length; }

describe("a call made without the array it used to build", () => {
  test("a lone spread is the list it iterates to", () => {
    const xs = [1, 2, 3];
    let s = 0;
    for (let i = 0; i < 1000; i++) s += f3(...xs);
    expect(s).toBe(123_000);
    expect(f3(...[4, 5, 6])).toBe(456);
    const holes = [1, , 3] as number[];
    expect(who.apply(null, holes)).toBe("undefined:1++3:3");
    expect(who(...holes)).toBe("undefined:1++3:3");
    expect(who(...new Set([7, 8]))).toBe("undefined:7+8:2");
    expect(who(...("ab" as any))).toBe("undefined:a+b:2");
    expect(who(...[])).toBe("undefined::0");
    let name = "";
    try { (f3 as any)(...(null as any)); } catch (e: any) { name = e.name; }
    expect(name).toBe("TypeError");
  });
  test("apply with a literal list passes its elements, and a receiver's own apply is its own", () => {
    let s = 0;
    for (let i = 0; i < 1000; i++) s = f3.apply(null, [s % 7, 1, 2]);
    expect(s % 100).toBe(12);
    expect(who.apply({ tag: "t" }, [1, 2])).toBe("t:1+2:2");
    expect(who.apply(null, [])).toBe("undefined::0");
    expect(who.apply({ tag: "u" }, [1, 2, 3, 4, 5])).toBe("u:1+2+3+4+5:5");
    expect(who.apply({ tag: "v" }, [undefined as any])).toBe("v::1");
    const own = { apply(_t: unknown, list: unknown[]) { return "own:" + list.length; } };
    expect((own as any).apply(null, [1, 2])).toBe("own:2");
    const log: string[] = [];
    const a = () => { log.push("a"); return 1; };
    const b = () => { log.push("b"); return 2; };
    expect(f3.apply(null, [a(), b(), 3])).toBe(123);
    expect(log.join()).toBe("a,b");
  });
  test("a bound function calls its target with the receiver and the arguments in order", () => {
    const bound = who.bind({ tag: "b" }, 1);
    let s = 0;
    for (let i = 0; i < 1000; i++) s += bound(i).length;
    expect(s > 0).toBe(true);
    expect(bound(2)).toBe("b:1+2:2");
    expect(bound(2, 3, 4)).toBe("b:1+2+3+4:4");
    expect(bound(2, 3, 4, 5)).toBe("b:1+2+3+4+5:5");
    expect(bound()).toBe("b:1:1");
    expect(bound.name).toBe("bound who");
    const plain = f3.bind(null);
    let t = 0;
    for (let i = 0; i < 1000; i++) t = plain(t % 9, 2, 1);
    expect(t % 100).toBe(21);
    class K { constructor(public v: number) {} }
    const BK = K.bind(null, 5) as unknown as new () => K;
    const made = new BK();
    expect(made.v).toBe(5);
    expect(made instanceof K).toBe(true);
    const viaNew = new (who.bind(null) as any)();
    expect(Object.getPrototypeOf(viaNew)).toBe(who.prototype);
  });
});

function plus(a: number, b: number): number { return a + b; }
function twice(x: number): number { return x * 2; }

describe("call and apply on a function the program proves", () => {
  test("are the call they spell, and the receiver still runs where it is not a name", () => {
    let s = 0;
    for (let i = 0; i < 1000; i++) s += plus.call(null, i, 1) + twice.apply(undefined, [i]);
    expect(s).toBe(499500 + 1000 + 999000);
    expect(plus.call(this, 1, 2)).toBe(3);
    expect(plus.apply(null, [4])).toBe(NaN);
    expect(plus.apply(null)).toBe(NaN);
    expect((twice as any).apply(null, [1, 2, 3])).toBe(2);
    const log: string[] = [];
    const t = () => { log.push("t"); return null; };
    expect(plus.call(t(), 1, 2)).toBe(3);
    expect(twice.apply(t(), [5])).toBe(10);
    expect(log.join()).toBe("t,t");
    const a = () => { log.push("a"); return 1; };
    const b = () => { log.push("b"); return 2; };
    expect(plus.call(null, a(), b())).toBe(3);
    expect(log.join()).toBe("t,t,a,b");
  });
});

describe("a spread of an array the function fixes", () => {
  test("is the reads it stands for, and grows back into a spread where the array grows", () => {
    const xs = [1, 2, 3];
    let s = 0;
    for (let i = 0; i < 1000; i++) s += f3(...xs);
    expect(s).toBe(123_000);
    const ys = [4, 5];
    expect(f3(...ys, 6)).toBe(456);
    expect(f3(0, ...ys)).toBe(45);
    const grown = [7, 8];
    grown.push(9);
    expect(f3(...grown)).toBe(789);
    const short = [1];
    expect(f3(...short)).toBe(NaN);
    const strs = ["a", "b"];
    expect(who(...(strs as any))).toBe("undefined:a+b:2");
    const log: string[] = [];
    const a = () => { log.push("a"); return 1; };
    const zs = [a(), a()];
    expect(f3(...zs, a())).toBe(111);
    expect(log.join()).toBe("a,a,a");
    const read = [10, 20];
    const other = read;
    other[0] = 30;
    expect(f3(...read, 0)).toBe(3200);
  });
});

describe("a bound function the body declares and keeps to itself", () => {
  test("calls its target with the partials first, and is still a bound function", () => {
    const inc = plus.bind(null, 1);
    let s = 0;
    for (let i = 0; i < 1000; i++) s += inc(i);
    expect(s).toBe(499500 + 1000);
    const tens = plus.bind(undefined, 10);
    expect(tens(5) + inc(1)).toBe(17);
    const none = twice.bind(null);
    expect(none(21)).toBe(42);
    expect((none as any)()).toBe(NaN);
    expect(typeof inc).toBe("function");
    expect(inc.name).toBe("bound plus");
    expect(inc.length).toBe(1);
    let early = "";
    try { (late as any)(1); } catch (e: any) { early = e.name; }
    const late = plus.bind(null, 2);
    expect(early).toBe("ReferenceError");
    expect(late(3)).toBe(5);
  });
});

describe("a spread of an array the function does not fix", () => {
  test("hands the array itself, and nothing the callee does reaches it", () => {
    const xs = [1, 2, 3];
    const keep = [xs];
    let s = 0;
    for (let i = 0; i < 1000; i++) s += f3(...xs);
    expect(s).toBe(123_000);
    function grows(...r: number[]): number { r.push(9); return r.length; }
    function writes(...r: number[]): number { (arguments as any)[0] = 100; r[1] = 200; return r[0] + r[1]; }
    expect(grows(...xs)).toBe(4);
    expect(xs.length).toBe(3);
    expect(writes(...xs)).toBe(201);
    expect(xs.join()).toBe("1,2,3");
    expect(keep[0]).toBe(xs);
    const holes = [1, , 3] as number[];
    expect(who(...holes)).toBe("undefined:1++3:3");
    const six = [1, 2, 3, 4, 5, 6];
    expect(who(...six)).toBe("undefined:1+2+3+4+5+6:6");
  });
});
