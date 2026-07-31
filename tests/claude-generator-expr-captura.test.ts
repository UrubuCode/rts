import { describe, test, expect } from "rts:test";

// Uma generator EXPRESSION aninhada que CAPTURA o escopo (`const g =
// function*(){ yield r(o.v) }` dentro de uma função) perdia as capturas: o
// parser levantava TODA generator expression para `__genexpr_N` no topo, onde
// `o`/`r` não existem mais. Dava `ReferenceError: o is not defined` no motor e
// `call to unknown function 'o'` nos bundles.
//
// A regra que reconcilia os dois casos: no TOPO do módulo as livres já SÃO
// globais e continuam alcançáveis, então levantar é sempre seguro — é por isso
// que ligar `sem_captura` incondicionalmente quebrava o caso do topo. Dentro de
// um bloco, quem captura é desugarado NO LUGAR (eager-buffer) e deixa de ser
// generator: vira uma fn-expression comum, que a maquinaria de closure já sabe
// extrair com as capturas.
//
// LIMITE mantido explícito: se o corpo usa `yield` em posição de VALOR
// (`const a = yield b`), o eager-buffer não o expressa — esse caso precisa da
// state-machine, que exige o hoist, então continua levantando (e continua
// perdendo a captura, como antes). Detectado checando se sobrou `yield` no corpo
// desugarado.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

function comCaptura() {
  const o = { v: 5 };
  function r(x) { return x + 1; }
  const g = function* () { yield r(o.v); };
  return g().next().value;
}
const viaNext = comCaptura();

function capturaSpread() {
  const base = 10;
  const g = function* () { yield base; yield base + 1; };
  return [...g()].join(",");
}
const viaSpread = capturaSpread();

function capturaForOf() {
  const arr = [1, 2, 3];
  const g = function* () { for (const x of arr) yield x * 2; };
  let s = 0;
  for (const v of g()) { s = s + v; }
  return s;
}
const viaForOf = capturaForOf();

function capturaParam(mult) {
  const g = function* () { yield 1 * mult; yield 2 * mult; };
  return [...g()].join(",");
}
const viaParam = capturaParam(3);

function capturaAninhadaDupla() {
  const a = 100;
  function meio() {
    const b = 20;
    const g = function* () { yield a + b; };
    return g().next().value;
  }
  return meio();
}
const viaAninhadaDupla = capturaAninhadaDupla();

// ── não-regressões ─────────────────────────────────────────────────────────
const noTopo = function* () { yield 7; };
const viaTopo = noTopo().next().value;

function semCaptura() {
  const g = function* () { yield 42; };
  return g().next().value;
}
const viaSemCaptura = semCaptura();

function* declaracao() { yield 1; yield 2; }
const viaDeclaracao = [...declaracao()].join(",");

describe("generator expression aninhada preserva a captura", () => {
  test(".next() com captura de objeto e função", () => expect(viaNext).toBe(6));
  test("spread com captura de const", () => expect(viaSpread).toBe("10,11"));
  test("for-of com captura de array", () => expect(viaForOf).toBe(12));
  test("captura de PARÂMETRO da função envolvente", () => expect(viaParam).toBe("3,6"));
  test("captura atravessando dois níveis", () => expect(viaAninhadaDupla).toBe(120));
});

describe("não-regressões", () => {
  test("generator expression no TOPO continua funcionando", () => expect(viaTopo).toBe(7));
  test("generator expression aninhada SEM captura", () => expect(viaSemCaptura).toBe(42));
  test("declaração de generator não regrediu", () => expect(viaDeclaracao).toBe("1,2"));
});
