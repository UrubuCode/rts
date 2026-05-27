import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #128 fase 2): `return` dentro de `try` com
// `finally` gerava IR INVALIDO ("terminator before end of block", crash de
// compilacao). Semantica JS: finally roda ANTES do return efetivar. Fix:
// finally_stack em FnCtx; return inlina os finally pendentes (mais interno
// primeiro) antes de emitir return_; lower_try_stmt trata blocos orfaos
// quando try (e catch) terminam com return.

let out = "";
function print(v: string): void { out += v + "\n"; }

// return em try com finally
function f(): string {
  try { return "try"; }
  finally { out += "fin\n"; }
}
print(f());            // fin, depois "try"

// multiplos returns no mesmo try (cada um roda o finally)
function g(x: number): number {
  try {
    if (x > 0) return x * 2;
    return -1;
  } finally { out += "gf\n"; }
}
print("" + g(5));      // gf, 10
print("" + g(-3));     // gf, -1

// try/catch/finally com return no try (caminho feliz)
function h(): string {
  try { return "ok"; }
  catch (e) { return "err"; }
  finally { out += "hf\n"; }
}
print(h());            // hf, ok

// finally roda mesmo sem return (caminho normal preservado)
function n(): number {
  let r = 0;
  try { r = 1; } finally { r = r + 10; }
  return r;
}
print("" + n());       // 11

describe("return in try finally", () => {
  test("finally roda antes do return", () =>
    expect(out).toBe("fin\ntry\ngf\n10\ngf\n-1\nhf\nok\n11\n"));
});
