import { describe, test, expect } from "rts:test";

// (#640) Default param referenciando outro param.
// `function rect(w, h = w)` -> callsite `rect(5)` deve substituir `w`
// no default `h = w` pelo arg da posicao de w (i.e. 5).

function rect(w: number, h: number = w): number { return w * h; }
function chain(a: number, b: number = a + 1, c: number = b * 2): number {
  return a + b + c;
}
function userDefault(x: number, y: number = x * 2, z: number = x + y): number {
  return z;
}

const r1 = rect(5);
const r2 = rect(5, 3);
const c1 = chain(1);
const c2 = chain(1, 10);
const c3 = chain(1, 10, 100);
const u1 = userDefault(3);

describe("Default param cross-reference (#640)", () => {
  test("rect(5) -> w*w = 25", () => expect(r1).toBe(25));
  test("rect(5, 3) -> 5*3 = 15", () => expect(r2).toBe(15));
  test("chain(1) cascateia b=a+1=2, c=b*2=4 -> 1+2+4 = 7", () =>
    expect(c1).toBe(7));
  test("chain(1, 10) -> c=10*2=20 -> 1+10+20 = 31", () => expect(c2).toBe(31));
  test("chain(1, 10, 100) explicit -> 1+10+100 = 111", () => expect(c3).toBe(111));
  test("userDefault(3) -> y=6, z=3+6=9", () => expect(u1).toBe(9));
});
