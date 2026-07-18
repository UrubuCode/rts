import { describe, test, expect } from "rts:test";
import { parse, stringify, escape, unescape, encode, decode } from "node:querystring";

// ---- parse basics ----------------------------------------------------------
const p1: any = parse("a=1&b=2&c=hello");
const p2: any = parse("k=1&k=2&k=3");           // repeated → array
const p3: any = parse("a&b=&c=1");              // missing value → ""
const p4: any = parse("");                       // empty → {}
const p5: any = parse("a:1;b:2", ";", ":");      // custom sep/eq

// ---- parse options ---------------------------------------------------------
const p6: any = parse("a=1&b=2&c=3", "&", "=", { maxKeys: 2 } as any);
const p7: any = parse("a=1&b=2&c=3&d=4", "&", "=", { maxKeys: 0 } as any);   // unlimited
const p8: any = parse("a=%41", "&", "=", { decodeURIComponent: (s: any) => "<" + s + ">" } as any);

// ---- stringify -------------------------------------------------------------
const s1 = stringify({ a: 1, b: "x y" });
const s2 = stringify({ a: [1, 2, 3], b: "z" });                  // array → repeated
const s3 = stringify({ a: 1, b: 2 }, ";", ":");                  // custom sep/eq
const s4 = stringify({ n: 42, t: true, f: false });              // number/bool
const s5 = stringify({ a: "x", b: "y" }, "&", "=", { encodeURIComponent: (v: any) => ("" + v).toUpperCase() } as any);

// ---- escape / unescape -----------------------------------------------------
const e1 = escape("a b&c=d");
const u1 = unescape("a%20b%26c");

// ---- decode / encode aliases -----------------------------------------------
const d1: any = decode("x=1&y=2");
const en1 = encode({ p: "q" });

describe("node:querystring", () => {
  test("parse basics", () => {
    expect(p1.a).toBe("1");
    expect(p1.b).toBe("2");
    expect(p1.c).toBe("hello");
    expect(Array.isArray(p2.k)).toBe(true);
    expect(p2.k.length).toBe(3);
    expect(p2.k[2]).toBe("3");
    expect(p3.a).toBe("");
    expect(p3.c).toBe("1");
    expect(Object.keys(p4).length).toBe(0);
    expect(p5.a).toBe("1");
    expect(p5.b).toBe("2");
  });
  test("parse maxKeys", () => {
    expect(Object.keys(p6).length).toBe(2);
    expect(Object.keys(p7).length).toBe(4);
  });
  test("parse custom decodeURIComponent (keys + values)", () => {
    expect(Object.keys(p8).join(",")).toBe("<a>");
    expect(p8["<a>"]).toBe("<%41>");
  });
  test("stringify", () => {
    expect(s1).toBe("a=1&b=x%20y");
    expect(s2).toBe("a=1&a=2&a=3&b=z");
    expect(s3).toBe("a:1;b:2");
    expect(s4).toBe("n=42&t=true&f=false");
    expect(s5).toBe("A=X&B=Y");
  });
  test("escape / unescape", () => {
    expect(e1).toBe("a%20b%26c%3Dd");
    expect(u1).toBe("a b&c");
  });
  test("decode / encode aliases", () => {
    expect(d1.x).toBe("1");
    expect(d1.y).toBe("2");
    expect(en1).toBe("p=q");
  });
});
