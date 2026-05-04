import { describe, test, expect } from "rts:test";

// (#480) Method shorthand: \`obj.add(n)\` em object literal nao e' mais
// interceptado por Set.add hijack. \`add\`/\`set\`/\`get\`/\`has\`/\`delete\`
// como method names em obj literais funcionam.

const calc = {
  v: 0,
  add(n: number) { (this as any).v += n; },
  reset() { (this as any).v = 0; },
};
calc.add(5);
const v_after_add = calc.v;
calc.add(3);
const v_after_add2 = calc.v;
calc.reset();
const v_after_reset = calc.v;

const set_method = {
  store: 0,
  set(n: number) { (this as any).store = n; },
  get(): number { return (this as any).store; },
};
set_method.set(42);
const stored = set_method.get();

describe("object_method_shorthand_simple", () => {
  test("obj.add(n) atualiza this.v", () => expect(v_after_add).toBe(5));
  test("obj.add chamadas multiplas", () => expect(v_after_add2).toBe(8));
  test("obj.reset()", () => expect(v_after_reset).toBe(0));
  test("obj.set/get com nome de Map.set", () => expect(stored).toBe(42));
});
