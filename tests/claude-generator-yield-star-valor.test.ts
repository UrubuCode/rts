import { describe, test, expect } from "rts:test";

// (bundle real) `yield*` em posição de VALOR — `const r = yield* g()`.
//
// A state-machine lowerava `yield*` só em posição de STATEMENT: o laço de
// delegação re-yielda cada valor da fonte, mas DESCARTAVA o valor de `return`
// dela. Em posição de valor o build caía no eager-buffer e o `yield` chegava
// cru ao lowering.
//
// Medido: dos 9 bundles da página (14,8 MB), esta é a ÚNICA das quatro formas
// restantes que aparece — 3 ocorrências, todas o `_loop` do Babel:
//
//   for (…) if (u = yield* s(), u) return u.v;
//   for (var u of e) s = yield* l();
//
// Async generator, `while (… yield …)`, `switch (… yield …)` e cabeçalho de
// `for` com yield: ZERO ocorrências.
//
// O valor de `yield* g()` é o `return` do DELEGADO (spec: o `value` do
// resultado `done:true` da fonte), NÃO o último valor yieldado — é o que
// separa "compila" de "certo" aqui.
//
// Três defeitos distintos foram necessários para chegar ao valor certo, cada um
// com seu teste abaixo:
//   1. o acessor de retorno existente lia só o mapa do caminho EAGER, keyed
//      pelo Vec ORIGINAL — mas a delegação de um Vec itera uma CÓPIA com
//      cursor, outro handle;
//   2. o default de "sem retorno" era o sentinela legado, que lido como
//      PolyValue vira o double `-1e-323` em vez de `undefined`;
//   3. `next(v)` não era encaminhado ao delegado, então um `const q = yield …`
//      DENTRO dele lia `undefined`.
//
// Todos os valores conferidos contra o Node. Pré-computado no top-level.

// ── delegado COM return ────────────────────────────────────────────────────
function* inner1() { yield 1; yield 2; return "RET"; }
function* outer1() { const r = yield* inner1(); return "got:" + r; }
const o1: any = outer1();
const comRetorno = JSON.stringify([o1.next(), o1.next(), o1.next()]);

// ── delegado SEM return → undefined (nao o sentinela legado) ───────────────
function* inner2() { yield 1; }
function* outer2() { const r = yield* inner2(); return "got:" + r; }
const o2: any = outer2();
const semRetorno = JSON.stringify([o2.next(), o2.next()]);

// ── delegar um ARRAY (fonte sem valor de retorno) ──────────────────────────
function* outer3() { const r = yield* [7, 8]; return "got:" + r; }
const o3: any = outer3();
const arrayFonte = JSON.stringify([o3.next(), o3.next(), o3.next()]);

// ── delegação ANINHADA: cada nível pega o retorno do seu delegado ──────────
function* leaf() { yield "L"; return "leafret"; }
function* mid() { const x = yield* leaf(); yield "M:" + x; return "midret"; }
function* top() { const y = yield* mid(); return "top:" + y; }
const o4: any = top();
const aninhada = JSON.stringify([o4.next(), o4.next(), o4.next()]);

// ── a forma EXATA do bundle: `if (u = yield* f(), u) return u.v` ───────────
// Sequência com atribuição no teste de um `if`, dentro de um `for`. O residual
// não-final da sequência TEM de virar statement: descartá-lo perdia o `u = …`
// e o teste lia o `u` velho (o generator devolvia "none" em vez de 42).
function* s1() { yield "a"; return { v: 42 }; }
function* s2() { yield "b"; return null; }
function* bundleShape(list: any) {
  let u;
  for (let c = 0; c < list.length; c++) {
    if (u = yield* list[c](), u) return u.v;
  }
  return "none";
}
const o5: any = bundleShape([s2, s1]);
const formaDoBundle = JSON.stringify([o5.next(), o5.next(), o5.next()]);

// ── controle: `yield*` em STATEMENT segue idêntico ─────────────────────────
function* stmtDeleg() { yield* [1, 2]; yield 3; }
const statementSegueIgual = JSON.stringify([...stmtDeleg()]);

// ── `next(v)` encaminhado ATRAVÉS da delegação ─────────────────────────────
function* innerSent() { const q = yield "ask"; return "inner-got:" + q; }
function* outerSent() { const r = yield* innerSent(); return r; }
const o7: any = outerSent();
const sentEncaminhado = JSON.stringify([o7.next(), o7.next("SENT")]);

describe("generator: yield* em posicao de valor", () => {
  test("delegado com return", () =>
    expect(comRetorno).toBe(
      '[{"value":1,"done":false},{"value":2,"done":false},{"value":"got:RET","done":true}]',
    ));
  test("delegado sem return da undefined", () =>
    expect(semRetorno).toBe(
      '[{"value":1,"done":false},{"value":"got:undefined","done":true}]',
    ));
  test("delegar array", () =>
    expect(arrayFonte).toBe(
      '[{"value":7,"done":false},{"value":8,"done":false},{"value":"got:undefined","done":true}]',
    ));
  test("delegacao aninhada", () =>
    expect(aninhada).toBe(
      '[{"value":"L","done":false},{"value":"M:leafret","done":false},{"value":"top:midret","done":true}]',
    ));
  test("forma exata do bundle (u = yield* f(), u)", () =>
    expect(formaDoBundle).toBe(
      '[{"value":"b","done":false},{"value":"a","done":false},{"value":42,"done":true}]',
    ));
  test("controle: yield* em statement", () => expect(statementSegueIgual).toBe("[1,2,3]"));
  test("next(v) encaminhado ao delegado", () =>
    expect(sentEncaminhado).toBe(
      '[{"value":"ask","done":false},{"value":"inner-got:SENT","done":true}]',
    ));
});
