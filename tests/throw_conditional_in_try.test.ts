import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #128 fase 2): `throw` LEXICO dentro de try (nao
// como ultima instrucao) nao interrompia o fluxo — `if(c) throw e; return x`
// executava o `return x` apos o throw. Fix: throw faz jump direto pro
// catch_block (criado antes do body), com flag catch_was_targeted pra
// distinguir try-terminado-por-throw (catch tem predecessor) de por-return
// (catch orfao). lower_throw_stmt marca o topo do catch_target_stack.

let out = "";
function print(v: string): void { out += v + "\n"; }

// throw condicional + codigo depois no try
function f(b: number): number {
  try {
    if (b === 0) throw new Error("zero");
    return 100 / b;
  } catch (e) { return -1; }
}
print("" + f(0));   // -1 (throw desvia, nao executa return 100/0)
print("" + f(2));   // 50

// throw condicional com varios statements depois
function g(n: number): string {
  try {
    if (n < 0) throw new Error("neg");
    const x = n * 2;
    return "val:" + x;
  } catch (e) { return "negative"; }
}
print(g(5));        // val:10
print(g(-1));       // negative

// throw incondicional (ultima instrucao) continua OK
function h(): string {
  try { throw new Error("boom"); } catch (e) { return "caught"; }
}
print(h());         // caught

// try/catch/finally com return (regressao guard #1232)
function k(): string {
  try { return "ok"; } catch (e) { return "err"; } finally { out += "kf\n"; }
}
print(k());         // kf, ok

describe("throw conditional in try", () => {
  test("throw lexico interrompe fluxo", () =>
    expect(out).toBe("-1\n50\nval:10\nnegative\ncaught\nkf\nok\n"));
});
