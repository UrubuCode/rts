import { describe, test, expect } from "rts:test";

// A binding declared fresh per pass of a loop and captured by a closure lives in an
// environment of its own per pass. One slot per activation would hand every closure
// the last pass's value; a `for (let ...)` head is also COPIED before each update, so
// a closure made in one pass keeps that pass's binding while the loop counts on.

function headLet(): string {
  const fs: Array<() => number> = [];
  for (let i = 0; i < 3; i++) fs.push(() => i);
  return fs.map((f) => f()).join();
}

function headLetWritten(): string {
  const fs: Array<() => number> = [];
  for (let i = 0; i < 3; i++) fs.push(() => i++);
  return fs.map((f) => f()).join() + "|" + fs.map((f) => f()).join();
}

function forOfConst(): string {
  const fs: Array<() => number> = [];
  for (const x of [1, 2, 3]) fs.push(() => x);
  return fs.map((f) => f()).join();
}

function forInConst(): string {
  const fs: Array<() => string> = [];
  for (const k in { p: 1, q: 2 }) fs.push(() => k);
  return fs.map((f) => f()).join();
}

function bodyConst(): string {
  const fs: Array<() => number> = [];
  let n = 0;
  while (n < 3) {
    const m = n * 10;
    fs.push(() => m + n);
    n++;
  }
  return fs.map((f) => f()).join();
}

function nested(outer: number): string {
  const fs: Array<() => number> = [];
  for (let i = 0; i < 2; i++) {
    for (let j = 0; j < 2; j++) fs.push(() => i * 10 + j + outer);
  }
  return fs.map((f) => f()).join();
}

function continued(): string {
  const fs: Array<() => number> = [];
  for (let i = 0; i < 3; i++) {
    if (i === 1) continue;
    fs.push(() => i);
  }
  return fs.map((f) => f()).join();
}

function spreadInBody(): string {
  const out: number[] = [];
  for (let r = 0; r < 2; r++) {
    const o: any = { a: r };
    const cp: any = { ...o };
    out.push(cp.a);
  }
  return out.join();
}

function arrowThis(this: any): string {
  const fs: Array<() => number> = [];
  for (let i = 0; i < 2; i++) fs.push(() => this.v + i);
  return fs.map((f) => f()).join();
}

describe("loop pass environments", () => {
  test("a for-let head, per pass", () => expect(headLet()).toBe("0,1,2"));
  test("a closure writing its pass's head binding", () =>
    expect(headLetWritten()).toBe("0,1,2|1,2,3"));
  test("a for-of const", () => expect(forOfConst()).toBe("1,2,3"));
  test("a for-in const", () => expect(forInConst()).toBe("p,q"));
  test("a const in a while body", () => expect(bodyConst()).toBe("3,13,23"));
  test("nested loops, each with its own", () => expect(nested(100)).toBe("100,101,110,111"));
  test("a continue still copies", () => expect(continued()).toBe("0,2"));
  test("an object spread helper reading the pass", () => expect(spreadInBody()).toBe("0,1"));
  test("an arrow's this across a pass environment", () =>
    expect(arrowThis.call({ v: 5 })).toBe("5,6"));
});
