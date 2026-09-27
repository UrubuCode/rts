import { describe, test, expect } from "rts:test";

// `o[k]` with a computed key reads through a site that remembers the last key and
// layout it saw. What must hold is that the site never answers from what it
// remembers once the object, the key or the layout has moved on.

function keysOverOneObject(): string {
  const o: any = { a: 1, b: 2, c: 3, d: 4 };
  const keys = ["a", "b", "c", "d", "a", "zz"];
  const out: unknown[] = [];
  for (let i = 0; i < keys.length; i++) out.push(o[keys[i]]);
  return JSON.stringify(out);
}

function layoutChangesUnderTheSite(): string {
  const o: any = { a: 1, b: 2 };
  const out: unknown[] = [];
  for (let i = 0; i < 6; i++) {
    out.push(o["a"] + ":" + o["b"]);
    if (i === 1) delete o.a;
    if (i === 2) o.a = 10;
    if (i === 3) Object.defineProperty(o, "b", { get: () => 99 });
  }
  return out.join();
}

function oneSiteManyReceivers(): string {
  const read = (x: any, k: any) => x[k];
  const proto = { inherited: "p" };
  const child = Object.create(proto);
  child.own = "c";
  const out: unknown[] = [];
  for (const k of ["own", "inherited", "length", "0", "missing"]) {
    out.push(read(child, k), read([7, 8], k), read("xy", k));
  }
  return JSON.stringify(out);
}

function typeofInALoop(): number {
  const things: unknown[] = [{}, null, [], () => 1, "s", 1];
  let n = 0;
  for (let i = 0; i < things.length; i++) if (typeof things[i] === "object") n++;
  return n;
}

function literalInALoop(): number {
  const words = ["abc", "abd", "abc"];
  let n = 0;
  for (let i = 0; i < words.length; i++) if (words[i] === "abc") n++;
  return n;
}

describe("computed key site", () => {
  test("keys over one object", () => {
    expect(keysOverOneObject()).toBe("[1,2,3,4,1,null]");
  });
  test("a layout that changes under the site", () => {
    expect(layoutChangesUnderTheSite()).toBe(
      "1:2,1:2,undefined:2,10:2,10:99,10:99",
    );
  });
  test("one site, many receivers", () => {
    expect(oneSiteManyReceivers()).toBe(
      '["c",null,null,"p",null,null,null,2,2,null,7,"x",null,null,null]',
    );
  });
  test("typeof in a loop", () => {
    expect(typeofInALoop()).toBe(3);
  });
  test("a string literal in a loop", () => {
    expect(literalInALoop()).toBe(2);
  });
});
