import { describe, test, expect } from "rts:test";

// `for await (x of xs)` writing an EXISTING binding. It was refused by name
// because the write-back lost the value at the loop's back edge; the MIR stage
// carries it as a block parameter. What the language says: every element is
// assigned in turn, the binding keeps the LAST one after the loop, a `return`
// inside leaves with the current one, and an empty source leaves it untouched.

async function* gen(): AsyncGenerator<number> { yield 1; yield 2; yield 3; }

async function all(xs: any): Promise<string> {
  let x: any = 0;
  const out: any[] = [];
  for await (x of xs) out.push(x);
  return `${out.join(",")}|${x}`;
}
async function first(xs: any): Promise<any> {
  let x: any = "none";
  for await (x of xs) return x;
  return x;
}

describe("for await over an existing binding", () => {
  test("assigns each element and keeps the last", async () => {
    expect(await all(gen())).toBe("1,2,3|3");
    expect(await all([7, 8])).toBe("7,8|8");
  });
  test("a return inside leaves with the current element", async () => {
    expect(await first(gen())).toBe(1);
  });
  test("an empty source leaves the binding untouched", async () => {
    expect(await first([])).toBe("none");
    expect(await all([])).toBe("|0");
  });
});
