import { describe, test, expect } from "rts:test";

// (#777) `Object.create(null).toString` retornava o proprio handle do
// obj em vez de 0/undefined. Bug raiz: o fallback em members.rs
// iterava GLOBAL_CLASS_SPECS procurando instance_method por nome de
// prop. `String.toString` (primeiro spec) era passthrough — retornava
// o handle direto. Fix: skip-list de metodos universais
// (toString/valueOf/hasOwnProperty/...) — esses devem cair em
// map_get_chain que retorna 0 (key ausente) em vez de virar dispatch
// arbitrario.

const o = Object.create(null);
const ts = (o as any).toString;
const tsEqO = (ts as any) === (o as any);
const tsZero = (ts as any) === 0;
const noProp = (o as any).nonexistent;
const tsTypeOf = typeof (o as any).toString;

describe("Object.create(null) sem prototype methods (#777)", () => {
  test("o.toString nao retorna o proprio handle", () => expect(tsEqO).toBe(false));
  test("o.toString eh 0 (key ausente)", () => expect(tsZero).toBe(true));
  test("o.nonexistent eh 0", () => expect((noProp as any) === 0).toBe(true));
  test("typeof o.toString nao eh 'object'", () =>
    expect(tsTypeOf === "object" ? "wrong" : "ok").toBe("ok"));
});
