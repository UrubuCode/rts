import { describe, test, expect } from "rts:test";

// Duas coisas, e a primeira é a correção de um VALOR ERRADO que o #2051
// introduziu.
//
// 1) BUG DO #2051: ele desugarava no lugar o generator-que-captura e checava
//    "sobrou `yield`?" DEPOIS do desugar para decidir se o caso era suportado.
//    Só que o eager-buffer NÃO deixa `yield` sobrando — ele reescreve todo
//    `yield X` para `__gen_buf.push(X)`, inclusive em posição de VALOR. Então
//    `const a = yield o.v` virava `const a = push(...)` silenciosamente e o
//    resultado saía errado (`5,NaN` onde o Node dá `5,20`), em vez de falhar.
//    Agora a decisão é tomada no corpo ORIGINAL, antes do desugar
//    (`usa_yield_como_valor`): `yield` em posição de STATEMENT é o `expr` direto
//    de um `ExprStmt`; qualquer outro é de valor.
//
// 2) O caso de VALOR + CAPTURA passa a funcionar. Ele parecia uma contradição
//    (yield-de-valor exige state-machine → que exige hoist → que perde captura),
//    e a saída é levantar com as CAPTURAS COMO PARÂMETROS, deixando no lugar um
//    wrapper comum que as repassa:
//
//      const g = function*(){ const a = yield o.v; };
//      // vira
//      function* __genexpr_N(o){ const a = yield o.v; }
//      const g = function(){ return __genexpr_N(o); };
//
//    O corpo já referencia as capturas por esses nomes, então nada é renomeado.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

function vyCaptura() {
  const o = { v: 5 };
  const g = function* () { const a = yield o.v; yield a * 2; };
  const it = g();
  return it.next().value + "," + it.next(10).value;
}
const comObjeto = vyCaptura();

function vyCapturaFn() {
  const mult = 3;
  function h(x) { return x + 1; }
  const g = function* () { const a = yield h(1); yield a * mult; };
  const it = g();
  return it.next().value + "," + it.next(4).value;
}
const comFuncaoEConst = vyCapturaFn();

function vyCapturaParam(p) {
  const g = function* () { const a = yield p; yield a + p; };
  const it = g();
  return it.next().value + "," + it.next(10).value;
}
const comParametro = vyCapturaParam(2);

// ── não-regressões ─────────────────────────────────────────────────────────
function capSemValueYield() {
  const base = 10;
  const g = function* () { yield base; yield base + 1; };
  return [...g()].join(",");
}
const semValueYield = capSemValueYield();

const noTopo = function* () { const a = yield 1; yield a + 1; };
const t = noTopo();
const topoValueYield = t.next().value + "," + t.next(5).value;

function semCaptura() {
  const g = function* () { yield 42; };
  return g().next().value;
}
const aninhadoSemCaptura = semCaptura();

function* declaracao() { const a = yield 1; yield a * 3; }
const d = declaracao();
const daDeclaracao = d.next().value + "," + d.next(4).value;

describe("yield em posição de VALOR dentro de generator que captura", () => {
  test("captura de objeto", () => expect(comObjeto).toBe("5,20"));
  test("captura de função e const", () => expect(comFuncaoEConst).toBe("2,12"));
  test("captura de parâmetro da envolvente", () => expect(comParametro).toBe("2,12"));
});

describe("não-regressões", () => {
  test("captura sem yield-de-valor (desugar no lugar)", () =>
    expect(semValueYield).toBe("10,11"));
  test("yield-de-valor no TOPO (sem captura)", () => expect(topoValueYield).toBe("1,6"));
  test("aninhado sem captura", () => expect(aninhadoSemCaptura).toBe(42));
  test("declaração de generator", () => expect(daDeclaracao).toBe("1,12"));
});
