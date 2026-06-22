import { describe, test, expect } from "rts:test";

// #217 A1.1 — `new WeakRef(target)` (Registry-class instantiation) + `deref()`
// devolvendo o target enquanto vivo. A semântica weak real (deref → undefined
// após coleta) é validada por unit test no runtime; aqui cobrimos o caminho
// vivo end-to-end pelo motor novo: new WeakRef + deref + identidade do target.

const o = { v: 7 };
const w = new WeakRef(o);
const d = w.deref();

describe("fixture:weakref_basic", () => {
  test("deref returns the same target while alive", () => {
    expect(d === o).toBe(true);
  });
});
