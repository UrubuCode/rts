import { describe, test, expect } from "rts:test";
function add(a: number, b: number): number { return a + b; }
const c = add.call(null, 3, 4);
const a = add.apply(null, [5, 6]);
describe("ca", () => {
  test("call", () => { expect(c).toBe(7); });
  test("apply", () => { expect(a).toBe(11); });
});
