import { describe, test, expect } from "rts:test";

// A body that reads only `arguments.length` and `arguments[e]` gets no
// arguments object at all: the two reads answer from the activation's slots.
// What this pins is that the answers are the object's — the count for every
// width, holes and `undefined` told apart, keys that are not indices, wide
// calls past the four slots — and that every OTHER use of the name still gets
// the real object.

function count(this: any): number { return arguments.length; }
function first(this: any): any { return (arguments as any)[0]; }
function at(this: any, i: any): any { return (arguments as any)[i]; }
function sum(this: any): number {
  let s = 0;
  for (let i = 0; i < arguments.length; i++) s += (arguments as any)[i];
  return s;
}
function keyed(this: any, k: any): any { return (arguments as any)[k]; }

describe("arguments read light", () => {
  test("length is the number of arguments written, for every width", () => {
    const c = count as any;
    expect(c()).toBe(0);
    expect(c(1)).toBe(1);
    expect(c(1, 2)).toBe(2);
    expect(c(1, 2, 3)).toBe(3);
    expect(c(1, 2, 3, 4)).toBe(4);
    expect(c(1, 2, 3, 4, 5)).toBe(5);
    expect(c(1, 2, 3, 4, 5, 6, 7, 8, 9)).toBe(9);
    expect(c(undefined)).toBe(1);
    expect(c(1, undefined)).toBe(2);
    expect(c(...[1, 2, 3])).toBe(3);
    expect(c.call(null, 1, 2)).toBe(2);
    expect(c.apply(null, [1, 2, 3, 4, 5, 6])).toBe(6);
  });
  test("an index answers the argument, past the slots too, and undefined beyond", () => {
    const f = first as any, a = at as any;
    expect(f(7)).toBe(7);
    expect(f()).toBe(undefined);
    expect(a(0, "x", "y")).toBe(0);
    expect(a(1, "x", "y")).toBe("x");
    expect(a(2, "x", "y")).toBe("y");
    expect(a(3, "x", "y")).toBe(undefined);
    expect(a(5, "x", "y", "z", "w", "v")).toBe("v");
    expect(a(7, 1, 2, 3, 4, 5, 6, 7, 8, 9)).toBe(7);
    expect(a(-1, "x")).toBe(undefined);
    expect(a(0.5, "x")).toBe(undefined);
    expect((sum as any)(1, 2, 3, 4, 5, 6, 7, 8)).toBe(36);
    expect((sum as any)()).toBe(0);
    let acc = 0;
    for (let i = 0; i < 1000; i++) acc += (sum as any)(i, 1);
    expect(acc).toBe(499500 + 1000);
  });
  test("a key that is not an index reads what the object has", () => {
    const k = keyed as any;
    expect(k("length", 1, 2)).toBe(3);
    expect(k("0", 1)).toBe("0");
    expect(k("1", 1)).toBe(1);
    expect(k("nope", 1)).toBe(undefined);
    expect(typeof k(Symbol.iterator)).toBe("function");
  });
  test("any other use of the name still gets the real object", () => {
    function spread(this: any): number[] { return [...arguments as any]; }
    function alias(this: any): any { const a = arguments; return a.length; }
    function tag(this: any): string { return Object.prototype.toString.call(arguments); }
    function written(this: any): number { (arguments as any)[0] = 9; return (arguments as any)[0]; }
    function arrow(this: any): number { return (() => arguments.length)(); }
    function isArr(this: any): boolean { return Array.isArray(arguments); }
    function pass(this: any): number { return count.apply(null, arguments as any); }
    function other(this: any): any { return (arguments as any).callee === undefined ? "strict" : "sloppy"; }
    expect((spread as any)(1, 2, 3)).toEqual([1, 2, 3]);
    expect((alias as any)(1, 2)).toBe(2);
    expect((tag as any)(1)).toBe("[object Arguments]");
    expect((written as any)(1)).toBe(9);
    expect((arrow as any)(1, 2, 3)).toBe(3);
    expect((isArr as any)(1)).toBe(false);
    expect((pass as any)(1, 2, 3, 4, 5)).toBe(5);
    expect(typeof (other as any)(1)).toBe("string");
  });
  test("nested functions do not confuse the outer read", () => {
    function outer(this: any): number {
      function inner(this: any): number { return arguments.length * 10; }
      return arguments.length + (inner as any)(1, 2);
    }
    expect((outer as any)(1)).toBe(21);
    function outer2(this: any): any {
      const held = (arguments as any)[0];
      function inner(this: any): any { return [...arguments as any].join(); }
      return held + ":" + (inner as any)(1, 2) + ":" + arguments.length;
    }
    expect((outer2 as any)("h", "x")).toBe("h:1,2:2");
  });
});
