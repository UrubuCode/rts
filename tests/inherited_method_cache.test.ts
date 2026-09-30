import { describe, test, expect } from "rts:test";

// A method reached THROUGH a built-in prototype is cached by the site that
// calls it, for arrays and strings, which record no prototype link and are
// discriminated by their type alone. What this pins is not the speed but what
// the cache must keep true while being fast: a plain object holding `length`
// shares an array's SHAPE and must not be recognised by a site warmed on an
// array; an array that grows a property, loses one, or is relinked keeps
// answering through the right prototype; and `Object.prototype` two links up
// is reached through `Array.prototype`, which records no link either.

function readAt(o: any) { return o.at; }
function callAt(o: any) { return o.at(0); }
function owns(o: any, k: string) { return o.hasOwnProperty(k); }

describe("a site warmed on an array", () => {
  test("does not recognise a plain object with the same shape", () => {
    const xs = [1, 2, 3];
    for (let i = 0; i < 5; i++) callAt(xs);
    const fake = { length: 3 } as any;
    expect(readAt(fake)).toBe(undefined);
    expect(typeof readAt(xs)).toBe("function");
    let threw = false;
    try { callAt(fake); } catch { threw = true; }
    expect(threw).toBe(true);
  });

  test("keeps answering after the array grows and shrinks a property", () => {
    const ys: any = [4, 5];
    for (let i = 0; i < 5; i++) callAt(ys);
    ys.tag = "t";
    expect(callAt(ys)).toBe(4);
    expect(ys.map((v: number) => v * 2).join(",")).toBe("8,10");
    delete ys.tag;
    expect(callAt(ys)).toBe(4);
    expect(ys.tag).toBe(undefined);
  });

  test("follows a relinked array to its new prototype", () => {
    const zs: any = [1];
    for (let i = 0; i < 5; i++) callAt(zs);
    Object.setPrototypeOf(zs, { at() { return "mine"; } });
    expect(callAt(zs)).toBe("mine");
    expect(Array.isArray(zs)).toBe(true);
  });

  test("reaches Object.prototype two links up", () => {
    const xs = [1, 2, 3];
    for (let i = 0; i < 5; i++) owns(xs, "length");
    expect(owns(xs, "length")).toBe(true);
    expect(owns(xs, "x")).toBe(false);
    expect(owns({ x: 1 }, "x")).toBe(true);
    expect(owns({ x: 1 }, "length")).toBe(false);
  });
});

describe("a site warmed on a string", () => {
  test("answers every string, and not a wrapper's own property", () => {
    const s = "abcdefghijklmnop".repeat(16);
    let a = 0;
    for (let i = 0; i < 5; i++) a += s.indexOf("p", 200);
    expect(a).toBe(5 * 207);
    expect("q".repeat(3).padStart(5, "-")).toBe("--qqq");
    expect("abc".at(0)).toBe("a");
    const w: any = new String("w");
    w.at = () => "own";
    expect(w.at(0)).toBe("own");
    expect("w".at(0)).toBe("w");
  });

  test("hasOwnProperty knows a string's length and code units", () => {
    expect("abc".hasOwnProperty("length")).toBe(true);
    expect("abc".hasOwnProperty("0")).toBe(true);
    expect("abc".hasOwnProperty("3")).toBe(false);
    expect("abc".hasOwnProperty("x")).toBe(false);
  });
});

describe("Array.prototype and species", () => {
  test("Array.prototype is still an array that inherits from Object.prototype", () => {
    expect(Array.isArray(Array.prototype)).toBe(true);
    expect(Array.prototype.length).toBe(0);
    expect(typeof Array.prototype.hasOwnProperty).toBe("function");
  });

  test("a subclass still maps into itself and a plain array into Array", () => {
    class Fancy extends Array {}
    const f = Fancy.from([1, 2, 3]);
    expect(f.map((v) => v) instanceof Fancy).toBe(true);
    expect([1, 2].map((v) => v) instanceof Fancy).toBe(false);
  });
});
