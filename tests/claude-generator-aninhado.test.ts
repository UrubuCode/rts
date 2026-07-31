import { describe, test, expect } from "rts:test";

// `function*` declarado DENTRO de outra função (ou dentro de `new Function`).
//
// Antes só funcionava no TOPO do módulo: o desugar de generator vive no parser
// (`generator_sm`/`generator_desugar`) e era aplicado apenas por `lower_decl`,
// que só roda sobre `module.body`. Um generator aninhado chegava ao lowering
// como `HirStmt::Raw` e morria em "unrecognized statement `function* g(){…}`".
//
// Num bundle minificado real isso derruba o ARQUIVO INTEIRO — e como cada
// bundle é atômico, um único generator aninhado custava 6 MB de aplicação.
//
// O hoister (`GenExprHoister`) agora desce em corpos de função. A condição que
// protege capturas continua: só se levanta um generator cujo corpo não
// referencia nada do escopo em que está (ver `sem_captura`); um que captura fica
// onde está e mantém a falha honesta.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

// ── generator no TOPO (não pode regredir) ──────────────────────────────────
function* topo() { yield 1; yield 2; }
const itTopo = topo();
const topoPrimeiro = itTopo.next().value;

// ── generator ANINHADO numa função ─────────────────────────────────────────
function comGenerator(): number {
  function* g() { yield 10; yield 20; }
  const it = g();
  return it.next().value;
}
const aninhadoV = comGenerator();

// ── dois níveis de aninhamento ─────────────────────────────────────────────
function nivel1(): number {
  function nivel2(): number {
    function* g() { yield 42; }
    return g().next().value;
  }
  return nivel2();
}
const doisNiveis = nivel1();

// ── consumir mais de um valor ──────────────────────────────────────────────
function somaDoGerador(): number {
  function* nums() { yield 1; yield 2; yield 3; }
  const it = nums();
  return it.next().value + it.next().value + it.next().value;
}
const soma = somaDoGerador();

// ── o iterador termina: o valor após o fim é `undefined` ───────────────────
// (o campo `.done` é lido como Tagged e ainda não coage para Bool — gap
// separado, fora do escopo deste teste; o `value` prova o mesmo.)
function terminaDireito(): any {
  function* um() { yield 7; }
  const it = um();
  it.next();
  return it.next().value;
}
const depoisDoFim = terminaDireito();

describe("generator aninhado", () => {
  test("no topo continua funcionando", () => {
    expect(topoPrimeiro).toBe(1);
  });

  test("dentro de uma função", () => {
    expect(aninhadoV).toBe(10);
  });

  test("dois níveis de aninhamento", () => {
    expect(doisNiveis).toBe(42);
  });

  test("consome vários valores em ordem", () => {
    expect(soma).toBe(6);
  });

  test("depois do último yield o value é undefined", () => {
    expect(depoisDoFim).toBe(undefined);
  });
});
