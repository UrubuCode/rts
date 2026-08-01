import { describe, test, expect } from "rts:test";

// Um `;` solto dentro de um generator abortava a construção da state-machine
// INTEIRA — o corpo caía no eager-buffer e o `yield` de valor chegava cru ao
// lowering. Repro mínimo:
//
//   function* g(t){ const a = yield t; ; return a; }
//   // → "expression raw/unrecognized: Yield(...)"
//
// A forma real que trouxe isto: o corpo VAZIO de um laço, que é como o
// minificador escreve iteração cujo trabalho todo está no cabeçalho —
// `for (var a = yield f(), i = a.length - 1; i >= 0 && g(a[i]); i--);`
// (ext4_22 da varredura dos bundles). `Stmt::Empty` não tinha arm em
// `lower_stmt` e caía no `_ => None`, que aborta o build.
//
// Correção: `Stmt::Empty` e `Stmt::Debugger` não produzem nada e devolvem o
// estado corrente.
//
// Valores conferidos contra o Node.

// `;` extra no meio do corpo
function* comPontoEVirgula(t: any) { const a = yield t; ; return a; }
const p1: any = comPontoEVirgula("T");
const pontoEVirgula = JSON.stringify([p1.next(), p1.next("V")]);

// laço de corpo vazio depois de um yield de valor
function* lacoVazio(t: any) {
  const a = yield t;
  let n = 0;
  for (let i = 0; i < 3; i++);
  return a + ":" + n;
}
const l1: any = lacoVazio("T");
const corpoVazio = JSON.stringify([l1.next(), l1.next("V")]);

// a forma do bundle: yield no init do for E corpo vazio
function* formaDoBundle(xs: any) {
  for (var a = yield xs, i = a.length - 1; i >= 0; i--);
  return "i=" + i + " len=" + a.length;
}
const b1: any = formaDoBundle(0);
const initEVazio = JSON.stringify([b1.next(), b1.next([1, 2, 3])]);

describe("generator: statement vazio", () => {
  test("`;` solto no corpo", () =>
    expect(pontoEVirgula).toBe('[{"value":"T","done":false},{"value":"V","done":true}]'));
  test("laco de corpo vazio", () =>
    expect(corpoVazio).toBe('[{"value":"T","done":false},{"value":"V:0","done":true}]'));
  test("yield no init com corpo vazio (forma do bundle)", () =>
    expect(initEVazio).toBe('[{"value":0,"done":false},{"value":"i=-1 len=3","done":true}]'));
});
