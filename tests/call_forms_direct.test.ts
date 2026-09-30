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
