import { describe, test, expect } from "rts:test";

// (bundle real, ext1) As três formas que ainda travavam o cluster `Yield`,
// isoladas dos 91 generators do menor bundle e reduzidas à mão.
//
// ── 1. `stmt_has_yield` não enxergava `Stmt::Try` ──────────────────────────
// O detector era um `match` por forma e não tinha arm para `try`. Então um
//
//   if (c) { try { … yield … } catch (e) {} finally { … } }
//
// respondia "sem yield", e o arm `Stmt::If` empurrava o `if` INTEIRO **verbatim**
// para dentro de um estado — com o `yield` junto, que só aparecia como
// `raw/unrecognized: Yield` no lowering. Era o g65 (186 B). Os dois detectores
// (`stmt_has_yield` e `expr_has_yield`) viraram VISITORS: uma forma esquecida
// num `match` produz sempre a mesma falha silenciosa, e um visitor não esquece.
//
// ── 2. um `try/catch` SEM yield abortava o generator INTEIRO ───────────────
// O caminho de estados exigia que houvesse yield no try ou no catch; sem isso
// caía no bail lá embaixo e TODO o corpo ia para o eager-buffer, onde os outros
// yields de valor viram `push`. Repro de três linhas:
//
//   function* g(e) { const a = yield e();
//                    try { return 1; } catch (x) {}
//                    return a; }
//
// Era o g78. O caminho já servia os dois casos: catch sem yield vira statements
// ordinários no estado do catch, e um `return` dentro do try vira DONE — que é
// justamente o que NÃO pode ir verbatim para um estado.
//
// ── 3. `yield` no INIT de um `for` ────────────────────────────────────────
// A normalização recusava o cabeçalho inteiro. Mas TESTE e UPDATE são
// reavaliados a cada volta (içar mudaria quantas vezes rodam) e o INIT roda
// EXATAMENTE UMA VEZ — movê-lo para antes do laço é idêntico em semântica. Era
// o g48: `for (var n = yield a(), m = yield b(n); c < s.length;)`.
//
// Todos os valores conferidos contra o Node.
//
// GAPS PRÉ-EXISTENTES medidos aqui e NÃO corrigidos (o binário release anterior
// a esta frente dá o mesmo resultado — não são regressão):
//   * `throw` dentro de um generator: a máquina não modela, o build baila;
//   * erro de RUNTIME dentro do try (`o.missing.deep`) não entra no `catch` do
//     generator — o Node devolve "err", o RTS devolve `{done:true}` sem valor;
//   * `finally` NÃO roda ao completar normalmente (`[...g()]` deixa o `finally`
//     sem executar).
// Os dois últimos são resposta errada em silêncio; ficam registrados aqui em
// vez de escondidos.

// ── forma do g65: try/catch/finally com yield DENTRO de um if ──────────────
const seen: any[] = [];
function* g65(v: any, ok: any) {
  let n = 0;
  if (ok) {
    try { seen.push("in"), yield v; } catch (x) { } finally { seen.push("fin"), n = n + 1; }
  }
  return "n=" + n + "|" + seen.join(",");
}
const a1: any = g65("X", true);
const tryDentroDeIf = JSON.stringify([a1.next(), a1.next()]);

// ── try/catch SEM yield em qualquer lugar do corpo ────────────────────────
function* semYield() { const a = yield "A"; try { return "early:" + a; } catch (x) { } return "late"; }
const b1: any = semYield();
const semYieldComReturn = JSON.stringify([b1.next(), b1.next("V")]);

function* semYield2() { const a = yield "A"; try { const z = 1; } catch (x) { } return "end:" + a; }
const c1: any = semYield2();
const semYieldSemReturn = JSON.stringify([c1.next(), c1.next("V")]);

// ── forma do g78: try/catch aninhado DENTRO do catch ──────────────────────
function* aninhado(e: any, t: any) {
  try { return yield e(); } catch (err) { try { return t(); } catch (e2) { return "inner"; } }
}
const d1: any = aninhado(() => "E", () => "T");
const tryAninhado = JSON.stringify([d1.next(), d1.next("V")]);

// ── `gen.throw()` continua entrando no catch ──────────────────────────────
function* comThrow() { try { const a = yield "A"; return "no:" + a; } catch (x) { return "yes:" + x; } }
const e1: any = comThrow();
const throwEntraNoCatch = JSON.stringify([e1.next(), e1.throw("BOOM")]);

// ── forma do g48: yield no INIT do for, dois declaradores ─────────────────
function* forInit(xs: any) {
  for (var n = yield "a", m = yield "b", i = 0; i < xs.length; i++) { yield xs[i] + n + m; }
  return "end";
}
const f1: any = forInit(["x", "y"]);
const yieldNoInit = JSON.stringify([
  f1.next(), f1.next("N"), f1.next("M"), f1.next(), f1.next(), f1.next(),
]);

// O init roda UMA vez só — `count` prova que `tick()` não foi reavaliado.
let count = 0;
function tick() { count = count + 1; return count; }
function* forInitUma() { for (var k = tick(), j = yield "s"; k < 3; k++) { } return "k=" + k + " count=" + count + " j=" + j; }
const g1: any = forInitUma();
const initRodaUmaVez = JSON.stringify([g1.next(), g1.next("J")]);

function* forInitBreak() { for (var n = yield "q", i = 0; i < 5; i++) { if (i === 2) break; yield i; } return n; }
const h1: any = forInitBreak();
const initComBreak = JSON.stringify([h1.next(), h1.next("Q"), h1.next(), h1.next()]);

// ── controle: `for` sem yield no init segue idêntico ──────────────────────
function* ctl() { for (let i = 0; i < 3; i++) { yield i; } }
const controleForSimples = JSON.stringify([...ctl()]);

describe("generator: try/catch e yield no init do for", () => {
  test("try/catch/finally com yield dentro de um if", () =>
    expect(tryDentroDeIf).toBe(
      '[{"value":"X","done":false},{"value":"n=1|in,fin","done":true}]',
    ));
  test("try/catch sem yield, com return", () =>
    expect(semYieldComReturn).toBe(
      '[{"value":"A","done":false},{"value":"early:V","done":true}]',
    ));
  test("try/catch sem yield, sem return", () =>
    expect(semYieldSemReturn).toBe(
      '[{"value":"A","done":false},{"value":"end:V","done":true}]',
    ));
  test("try/catch aninhado dentro do catch", () =>
    expect(tryAninhado).toBe('[{"value":"E","done":false},{"value":"V","done":true}]'));
  test("gen.throw() entra no catch", () =>
    expect(throwEntraNoCatch).toBe(
      '[{"value":"A","done":false},{"value":"yes:BOOM","done":true}]',
    ));
  test("yield no init do for, dois declaradores", () =>
    expect(yieldNoInit).toBe(
      '[{"value":"a","done":false},{"value":"b","done":false},{"value":"xNM","done":false},{"value":"yNM","done":false},{"value":"end","done":true},{"done":true}]',
    ));
  test("o init do for roda uma vez so", () =>
    expect(initRodaUmaVez).toBe(
      '[{"value":"s","done":false},{"value":"k=3 count=1 j=J","done":true}]',
    ));
  test("init com yield mais break", () =>
    expect(initComBreak).toBe(
      '[{"value":"q","done":false},{"value":0,"done":false},{"value":1,"done":false},{"value":"Q","done":true}]',
    ));
  test("controle: for sem yield no init", () => expect(controleForSimples).toBe("[0,1,2]"));
});
