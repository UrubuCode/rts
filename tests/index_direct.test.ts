import { describe, test, expect } from "rts:test";

// `a[i]` with an index the compiler proved a number reads an array's element or
// a typed array's value through one direct entry, and `s.charCodeAt(i)` reads
// the code unit the same way. Everything else — a key that is not a canonical
// index, a receiver that is neither, a proxy, a wrapper, an object with a
// member of the same name — takes the generic read or the member. What this
// pins is that the answers are the language's in every one of those shapes.

describe("indexed reads over a proven number", () => {
  test("an array answers its elements, holes and what is past the end", () => {
    const xs = [10, 20, 30, 40];
    let sum = 0;
    for (let i = 0; i < 4000; i++) sum += xs[i & 3];
    expect(sum).toBe(1000 * 100);
    const holes: any[] = [1, , 3];
    let seen = "";
    for (let i = 0; i < 5; i++) seen += String(holes[i]) + ",";
    expect(seen).toBe("1,undefined,3,undefined,undefined,");
    const mixed: any[] = ["a", { k: 1 }, null, undefined, 1.5, [7]];
    let kinds = "";
    for (let i = 0; i < mixed.length; i++) kinds += typeof mixed[i] + ",";
    expect(kinds).toBe("string,object,object,undefined,number,object,");
    expect(mixed[5][0]).toBe(7);
  });
  test("an index that is not a canonical one reads the property it names", () => {
    const xs: any = [10, 20, 30];
    xs[-1] = "minus";
    xs[1.5] = "frac";
    const out: any[] = [];
    for (const k of [-1, 1.5, NaN, -0, 2 ** 32, 1e21, Infinity]) out.push(xs[k]);
    expect(out[0]).toBe("minus");
    expect(out[1]).toBe("frac");
    expect(out[2]).toBe(undefined);
    expect(out[3]).toBe(10);
    expect(out[4]).toBe(undefined);
    expect(out[5]).toBe(undefined);
    expect(out[6]).toBe(undefined);
    let i = 0;
    i = i - 1;
    expect(xs[i]).toBe("minus");
    expect(xs[i + 2.5]).toBe("frac");
  });
  test("typed arrays answer their values and undefined past the end", () => {
    const f = new Float64Array([1.5, 2.5, 3.5]);
    const u = new Uint8Array([250, 251, 252, 253]);
    const big = new Int32Array([-1, 2 ** 31 - 1]);
    let a = 0;
    for (let i = 0; i < 3000; i++) a += f[i % 3] + u[i & 3];
    expect(a).toBe(1000 * 7.5 + 750 * (250 + 251 + 252 + 253));
    let k = 3;
    expect(f[k]).toBe(undefined);
    expect(u[k + 1]).toBe(undefined);
    expect(big[k - 3]).toBe(-1);
    expect(big[k - 2]).toBe(2147483647);
    expect(f[k - 3.5]).toBe(undefined);
  });
  test("a receiver that is neither still reads what it has", () => {
    const s = "héllo";
    const o: any = { 0: "zero", 1: "one", length: 2 };
    const p: any = new Proxy([1, 2, 3], { get: (t, key) => (key === "1" ? "trapped" : (t as any)[key]) });
    function args(this: any): any { let i = 1; i = i - 1; return [...(arguments as any)][i + 1]; }
    let got = "";
    for (let i = 0; i < 2; i++) got += s[i] + o[i] + String(p[i]) + ";";
    expect(got).toBe("hzero1;éonetrapped;");
    expect((args as any)("a", "b")).toBe("b");
    let n = 1;
    n = n + 0;
    expect((5 as any)[n]).toBe(undefined);
    expect(("ab" as any)[n + 5]).toBe(undefined);
  });
  test("charCodeAt reads the unit, and NaN outside the text", () => {
    const s = "aé€𝄞";
    let sum = 0;
    for (let i = 0; i < s.length; i++) sum += s.charCodeAt(i);
    expect(sum).toBe(97 + 233 + 8364 + 0xd834 + 0xdd1e);
    let k = 0;
    k = k - 1;
    expect(Number.isNaN(s.charCodeAt(k))).toBe(true);
    expect(Number.isNaN(s.charCodeAt(k + 100))).toBe(true);
    expect(s.charCodeAt(k + 1.9)).toBe(97);
    expect(s.charCodeAt(NaN)).toBe(97);
    expect((s as any).charCodeAt()).toBe(97);
    expect((s as any).charCodeAt("1")).toBe(233);
    expect((s as any).charCodeAt({ valueOf: () => 2 })).toBe(8364);
    let acc = 0;
    const word = "abcdefghijklmnop";
    for (let i = 0; i < 16000; i++) acc += word.charCodeAt(i & 15);
    expect(acc).toBe(1000 * (16 * 97 + 120));
  });
  test("charCodeAt on something that is not a string takes its member", () => {
    const boxed: any = new String("xyz");
    expect(boxed.charCodeAt(1)).toBe(121);
    const impostor: any = { charCodeAt: (i: number) => i * 2 };
    expect(impostor.charCodeAt(21)).toBe(42);
    const n: any = 5;
    let message = "";
    try { n.charCodeAt(0); } catch (e: any) { message = e.message; }
    expect(message.endsWith("is not a function")).toBe(true);
    let order = "";
    const recv = () => { order += "r"; return "ab"; };
    const idx = () => { order += "i"; return 1; };
    expect(recv().charCodeAt(idx())).toBe(98);
    expect(order).toBe("ri");
  });
});
