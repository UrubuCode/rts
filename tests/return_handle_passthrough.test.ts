import { describe, test, expect } from "rts:test";

// Regression: `return <objeto/handle>` numa fn sem anotacao de retorno
// (inferida como Handle) deve propagar o handle CRU, sem stringificar via
// TPL_COERCE_AUTO. Antes, `return (globalThis as any)[key]` e similares
// corrompiam o objeto, e o acesso a campo no caller dava 0.

// (1) ensureGlobal pattern (bundler/polyfill) — globalThis[key] dinamico.
function ensureGlobal(key: string, factory: () => any) {
  if (!(key in globalThis)) {
    (globalThis as any)[key] = factory();
  }
  return (globalThis as any)[key];
}
const libA = ensureGlobal("__rtsLibA", () => ({ version: "1.0", ready: true }));
const libA2 = ensureGlobal("__rtsLibA", () => ({ version: "2.0", ready: false }));

// (2) return direto de computed-get de globalThis (sem `as any`).
(globalThis as any)["__rtsK"] = { tag: "obj-k" };
function getK(key: string) {
  return globalThis[key];
}
const k = getK("__rtsK");

// (3) return de objeto vindo de member access, fn sem anotacao.
function pick(): any {
  return { name: "picked", n: 42 };
}
const p = pick();

describe("return handle passthrough", () => {
  test("ensureGlobal retorna instancia com campos intactos", () =>
    expect(libA.version).toBe("1.0"));
  test("ensureGlobal nao substitui na 2a chamada", () =>
    expect(libA2.version).toBe("1.0"));
  test("return globalThis[key] propaga objeto cru", () =>
    expect(k.tag).toBe("obj-k"));
  test("return de objeto literal em fn :any", () =>
    expect(p.name).toBe("picked"));
  test("campo numerico do objeto retornado", () =>
    expect(p.n).toBe(42));
});
