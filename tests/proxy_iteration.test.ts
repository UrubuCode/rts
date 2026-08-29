import { describe, test, expect } from "rts:test";

// Everything that iterates reaches `Iterate`, and `Iterate` looked up
// `Symbol.iterator` with a raw shape read — no proxy asked — so a proxy over
// anything iterable was refused as "not iterable". Reading an index through a
// proxy with no `get` trap had the mirror defect: it walked the target's shape
// properties, which an array's elements are not.

function answer(fn: () => unknown): string {
  try {
    return "" + JSON.stringify(fn());
  } catch (e: any) {
    return "THREW: " + (e && e.message);
  }
}

const target = [1, 2, 3];
const bare = () => new Proxy(target, {}) as any;

describe("fixture:proxy_iteration", () => {
  test("an index reads through a proxy that has no get trap", () => {
    expect(answer(() => bare()[0])).toBe("1");
    expect(answer(() => bare()["1"])).toBe("2");
    expect(answer(() => bare().length)).toBe("3");
    // A trap that is not `get` must not accidentally repair it.
    expect(answer(() => (new Proxy(target, { has: () => true }) as any)[0])).toBe("1");
  });

  test("the canonical rule still decides what an index is", () => {
    // `p["01"]` and `p["1.0"]` are ordinary properties on the target too, so
    // reading them through the proxy must not find elements either.
    expect(answer(() => bare()["01"])).toBe("undefined");
    expect(answer(() => bare()["1.0"])).toBe("undefined");
  });

  test("every construct that iterates accepts a proxy", () => {
    expect(answer(() => { const out: number[] = []; for (const v of bare()) out.push(v); return out; })).toBe("[1,2,3]");
    expect(answer(() => [...bare()])).toBe("[1,2,3]");
    expect(answer(() => ((...a: number[]) => a)(...bare()))).toBe("[1,2,3]");
    expect(answer(() => { function* h() { yield* bare(); } return [...h()]; })).toBe("[1,2,3]");
    expect(answer(() => [...new Set(bare())])).toBe("[1,2,3]");
    expect(answer(() => Array.from(bare()))).toBe("[1,2,3]");
    expect(answer(() => { const [a, b, c] = bare(); return [a, b, c]; })).toBe("[1,2,3]");
  });

  test("a proxy over a non-array iterable is accepted too", () => {
    // This is what rules out "Array.prototype's method was replaced" as the
    // explanation: the method is the target's OWN, and it was still refused.
    expect(answer(() => [...new Proxy({ *[Symbol.iterator]() { yield 7; yield 8; } }, {})])).toBe("[7,8]");
  });

  test("a get trap still sees every read the language makes", () => {
    const seen: string[] = [];
    const watched = new Proxy(target, {
      get(o: any, k: any, r: any) { seen.push(String(k)); return Reflect.get(o, k, r); },
    });
    const out: number[] = [];
    for (const v of watched) out.push(v);
    expect(JSON.stringify(out)).toBe("[1,2,3]");
    expect(seen[0]).toBe("@@iterator");
    expect(seen.includes("0")).toBe(true);
    expect(seen.includes("length")).toBe(true);
  });
});
