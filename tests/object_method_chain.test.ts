import { describe, test, expect } from "rts:test";

// (#480 chain) Method chain em obj literal: c.add(5).add(3).
// Receiver e' Call result com handle do Map (return this).

const c = {
  v: 0,
  add(n: number) { (this as any).v += n; return this; },
  sub(n: number) { (this as any).v -= n; return this; },
};
c.add(10);
const after_add = c.v;
c.add(5).add(3);
const after_chain = c.v;
c.add(20).sub(5).add(2);
const after_long_chain = c.v;

describe("object_method_chain", () => {
  test("add unico", () => expect(after_add).toBe(10));
  test("chain .add(5).add(3)", () => expect(after_chain).toBe(18));
  test("chain .add.sub.add", () => expect(after_long_chain).toBe(35));
});
