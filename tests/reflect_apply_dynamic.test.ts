import { describe, test, expect } from "rts:test";
function add(a: number, b: number): number { return a + b; }
const arr = [3, 4];
const c = add.apply(null, arr);
const r = Reflect.apply(add, null, [5, 6]);
describe("d", () => {
  test("dynamic apply", () => { expect(c).toBe(7); });
  test("reflect apply", () => { expect(r).toBe(11); });
});
