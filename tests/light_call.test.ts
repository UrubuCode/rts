import { describe, test, expect } from "rts:test";

// A call to a function that reads none of its activation's argument record is
// made by the compiled code itself; every other call takes the runtime's door.
// What this pins is that a program cannot tell which one it got: receivers,
// closures, defaults, recursion, throws and traces answer the same, and every
// callee the light path must refuse — `arguments`, a rest parameter, a fifth
// parameter, `new.target`, `async`, a generator, a class, a bound function, a
// proxy, a native, something not callable — still does what the language says.

function plain(a: number, b: number): number { return a * 10 + b; }
function defaults(a: number, b = 5, c = a + b): number { return a + b + c; }
function counts(this: any): number { return arguments.length; }
function rest(...xs: number[]): number { return xs.length; }
function five(a: number, b: number, c: number, d: number, e: number): number { return a + b + c + d + e; }
function target(this: any): string { return new.target === undefined ? "called" : "constructed"; }
function receiver(this: any): any { return this; }
function thrower(n: number): number { if (n > 2) throw new RangeError("too big: " + n); return n; }
function depth(n: number): number { return n === 0 ? 0 : 1 + depth(n - 1); }
function tail(n: number, acc: number): number { return n === 0 ? acc : tail(n - 1, acc + 1); }

describe("a call made directly", () => {
  test("plain functions, defaults, and fewer or more arguments than parameters", () => {
    let sum = 0;
    for (let i = 0; i < 2000; i++) sum += plain(i, 1);
    expect(sum).toBe(10 * (1999 * 2000) / 2 + 2000);
    expect(defaults(1)).toBe(1 + 5 + 6);
    expect(defaults(1, 2)).toBe(1 + 2 + 3);
    expect(defaults(1, undefined, 4)).toBe(1 + 5 + 4);
    expect((plain as any)(7)).toBe(NaN);
    expect((plain as any)(1, 2, 3, 4, 5, 6)).toBe(12);
  });
  test("a receiver travels, and a plain call has none", () => {
    const o = { k: 3, m(this: any, x: number) { return this.k + x; } };
    class C { v = 4; get(): number { return this.v; } add(x: number): number { return this.v + x; } }
    const c = new C();
    let a = 0;
    for (let i = 0; i < 1000; i++) a += o.m(i) + c.get() + c.add(1);
    expect(a).toBe(3000 + 499500 + 4000 + 5000);
    expect(receiver()).toBe(undefined);
    expect(receiver.call("s")).toBe("s");
    const held = o.m;
    let message = "";
    try { (held as any)(1); } catch (e: any) { message = e.message; }
    expect(message.length > 0).toBe(true);
  });
  test("closures read what they captured, through any number of calls", () => {
    const make = (k: number) => (x: number) => x + k;
    const add5 = make(5), add7 = make(7);
    let a = 0;
    for (let i = 0; i < 1000; i++) a += add5(i) - add7(i);
    expect(a).toBe(-2000);
    let shared = 0;
    const bump = () => { shared += 1; return shared; };
    for (let i = 0; i < 500; i++) bump();
    expect(shared).toBe(500);
  });
  test("recursion, and a tail call deep enough to need its frame reused", () => {
    expect(depth(500)).toBe(500);
    expect(tail(200_000, 0)).toBe(200_000);
  });
  test("a throw crosses the call, a finally runs, and the trace names the frames", () => {
    let caught = "", ran = 0;
    try { thrower(1); thrower(3); } catch (e: any) { caught = e.name + ":" + e.message; } finally { ran++; }
    expect(caught).toBe("RangeError:too big: 3");
    expect(ran).toBe(1);
    function inner(): string { return String(new Error("here").stack); }
    // NOT `return inner()`: a call in tail position gives up its frame, which is
    // the language's own rule for a trace taken under one.
    function outer(): string { const taken = inner(); return taken + ""; }
    const stack = outer();
    expect(stack.includes("at inner")).toBe(true);
    // `outer` is a frame where the function is called, and no frame where the
    // compiler spliced its body into the caller; where it is one, it is below.
    if (stack.includes("at outer")) expect(stack.indexOf("at inner") < stack.indexOf("at outer")).toBe(true);
    let after = 0;
    for (let i = 0; i < 100; i++) { try { thrower(i % 5); after++; } catch { after += 100; } }
    expect(after).toBe(60 + 40 * 100);
  });
  test("what reads the argument record still takes the door", () => {
    const c = counts as any;
    expect(c()).toBe(0);
    expect(c(1, undefined)).toBe(2);
    expect(c(1, 2, 3, 4, 5, 6)).toBe(6);
    expect(rest()).toBe(0);
    expect(rest(1, 2, 3, 4, 5, 6, 7)).toBe(7);
    expect(five(1, 2, 3, 4, 5)).toBe(15);
    expect(target()).toBe("called");
    expect(new (target as any)() instanceof target).toBe(true);
    function inConstructor(this: any) { this.seen = target(); }
    expect(new (inConstructor as any)().seen).toBe("called");
  });
  test("what is not a plain function answers what it always did", async () => {
    class K { constructor(public v: number) {} }
    let refused = "";
    try { (K as any)(1); } catch (e: any) { refused = e.message; }
    expect(refused.includes("new")).toBe(true);
    const bound = plain.bind(null, 4);
    expect(bound(2)).toBe(42);
    const proxied = new Proxy(plain, { apply: (f, _t, args) => f(args[0], args[1]) + 1 });
    expect(proxied(1, 2)).toBe(13);
    expect(Math.max(1, 9, 3)).toBe(9);
    expect([3, 1, 2].sort().join()).toBe("1,2,3");
    async function later(x: number): Promise<number> { return x + 1; }
    expect(await later(1)).toBe(2);
    function* gen() { yield 1; yield 2; }
    expect([...gen()].join()).toBe("1,2");
    const notCallable: any = { a: 1 };
    let named = "";
    try { notCallable.missing(1); } catch (e: any) { named = e.message; }
    expect(named.endsWith("is not a function")).toBe(true);
    const five: any = 5;
    let kind = "";
    try { five(); } catch (e: any) { kind = e.message; }
    expect(kind.endsWith("is not a function")).toBe(true);
  });
  test("arguments are evaluated once, in order, before the call", () => {
    const log: string[] = [];
    const a = () => { log.push("a"); return 1; };
    const b = () => { log.push("b"); return 2; };
    const f = () => { log.push("f"); return plain; };
    expect(f()(a(), b())).toBe(12);
    expect(log.join()).toBe("f,a,b");
  });
});
