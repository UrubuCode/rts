import { describe, test, expect } from "rts:test";

// Chamada de método com 4+ argumentos — issue #2039.
//
// A ABI do thunk uniforme leva QUATRO argumentos em registrador (`a0..a3`) e o
// resto num array de overflow que o corpo lê como `rest[pos - 4]`. Esse `pos`
// conta os slots do PRÓPRIO callee — e um callee this-first gasta o slot 0 com o
// receptor. Logo a MESMA chamada `o.m(1,2,3,4)` precisa do `4` em `a3` para um
// callee comum e em `rest[0]` para um this-first.
//
// Cada invoker fixava UM dos dois layouts. `__rtsadp_fn_invoke_method` reservava
// o slot 0 pro `this` e forçava `a3 = undefined`, então um callee COMUM alcançado
// por ele lia o 4º parâmetro de um slot vazio e o 5º de onde o 4º deveria estar:
//
//     o.m.apply(o, [1,2,3,4,5])   →   [1,2,3,undefined,4]
//
// Sem erro, sem crash — número errado e plausível seguindo programa afora, que é
// a classe de bug mais cara deste repo. O invoker simples (`__rtsadp_fn_invoke`)
// tinha o espelho: deslocava os posicionais pra abrir espaço ao `this` e
// DESCARTAVA o `a3` empurrado pra fora.
//
// Correção: os invokers param de fixar layout. Montam a lista lógica completa de
// slots (bound args, depois o receptor se o callee é this-first, depois os
// argumentos) e `adapters::value::argslots::pack` corta na fronteira
// registrador/overflow onde AQUELE callee espera.
//
// O teto de 3 argumentos sobre receiver-expressão (`g().m(a,b,c)`) caiu junto —
// ele era o bail honesto enquanto o invoker errava; agora não há o que evitar.
//
// Valores conferidos contra o Node. Pré-computado no top-level (regra do
// projeto: chamar método dentro de `test()` pode perder handle pro GC).
//
// NOTA — os testes passam SEMPRE o número exato de parâmetros que a função
// declara. Argumento ausente ainda renderiza `0` em vez de `undefined` em alguns
// slots (divergência PRÉ-EXISTENTE de repr do parâmetro, sem relação com
// descarte de argumento); asseverar sobre ela aqui mascararia o que este arquivo
// de fato cobre.

function id(x) {
  return x;
}
function j4(a, b, c, d) {
  return a + "," + b + "," + c + "," + d;
}
function j5(a, b, c, d, e) {
  return a + "," + b + "," + c + "," + d + "," + e;
}

// Receptor com um método `function` (enxerga o receptor como `this`) e outro que
// NÃO usa `this` — os dois layouts de callee, o mesmo call site.
const alvo = {
  tag: "T",
  comThis4: function (a, b, c, d) {
    return this.tag + ":" + j4(a, b, c, d);
  },
  comThis5: function (a, b, c, d, e) {
    return this.tag + ":" + j5(a, b, c, d, e);
  },
  semThis4: function (a, b, c, d) {
    return "P:" + j4(a, b, c, d);
  },
  semThis5: function (a, b, c, d, e) {
    return "P:" + j5(a, b, c, d, e);
  },
};

function pega() {
  return alvo;
}

// ── receiver-IDENTIFICADOR (o que já passava com ≤3 args, trava de regressão) ─
const identThis4 = alvo.comThis4(1, 2, 3, 4);
const identThis5 = alvo.comThis5(1, 2, 3, 4, 5);
const identSem4 = alvo.semThis4(1, 2, 3, 4);
const identSem5 = alvo.semThis5(1, 2, 3, 4, 5);

// ── receiver-EXPRESSÃO: chamada de função, `||`, índice, elemento de array ────
const exprChamada4 = pega().comThis4(1, 2, 3, 4);
const exprChamada5 = pega().comThis5(1, 2, 3, 4, 5);
const exprId4 = id(alvo).comThis4(1, 2, 3, 4);
const exprIdSem4 = id(alvo).semThis4(1, 2, 3, 4);
const exprIdSem5 = id(alvo).semThis5(1, 2, 3, 4, 5);
const exprOu4 = (null || alvo).comThis4(1, 2, 3, 4);
const exprIdx4 = id(alvo)["comThis4"](1, 2, 3, 4);
const exprIdx5 = id(alvo)["comThis5"](1, 2, 3, 4, 5);
const arrRecv = [alvo];
const exprArr4 = arrRecv[0].comThis4(1, 2, 3, 4);

// ── `.apply` / `.call` — a forma citada na issue ─────────────────────────────
const applyThis4 = alvo.comThis4.apply(alvo, [1, 2, 3, 4]);
const applyThis5 = alvo.comThis5.apply(alvo, [1, 2, 3, 4, 5]);
const applySem4 = alvo.semThis4.apply(alvo, [1, 2, 3, 4]);
const applySem5 = alvo.semThis5.apply(alvo, [1, 2, 3, 4, 5]);
const callThis4 = alvo.comThis4.call(alvo, 1, 2, 3, 4);
const callSem4 = alvo.semThis4.call(alvo, 1, 2, 3, 4);
const callSem5 = alvo.semThis5.call(alvo, 1, 2, 3, 4, 5);

// `.apply(null, …)` numa fn this-first SEM receptor: `this` fica `undefined`,
// os posicionais deslocam — e o 4º argumento (que o deslocamento empurra pra
// fora dos registradores) tem que sobreviver no overflow.
function semReceptor(a, b, c, d) {
  return "N:" + j4(a, b, c, d);
}
const semReceptor4 = semReceptor.apply(null, [1, 2, 3, 4]);
const semReceptorDireto = semReceptor(1, 2, 3, 4);

// ── `...spread` sobre receiver-expressão (o mesmo trampolim de apply) ─────────
const xs5 = [1, 2, 3, 4, 5];
const spreadExpr5 = id(alvo).comThis5(...xs5);
const spreadSem5 = id(alvo).semThis5(...xs5);

// ── `.bind` com aplicação parcial: os args ligados deslocam os do call site ──
// (mesma família — o deslocamento empurrava argumentos pra fora e eles sumiam).
const ligado1 = alvo.semThis4.bind(null, 1);
const bind1de4 = ligado1(2, 3, 4);
const ligado2 = alvo.semThis5.bind(null, 1, 2);
const bind2de5 = ligado2(3, 4, 5);

// ── semântica de `this` que NÃO pode quebrar ────────────────────────────────
// Campo-ARROW mantém o `this` léxico (não vê o receptor); campo `function`
// enxerga o receptor. Os dois, com 4 argumentos, sobre receiver-expressão.
function fabricaComArrow() {
  const lexico = { marca: "LEX" };
  const self = {
    marca: "RECV",
    arrow4: (a, b, c, d) => lexico.marca + ":" + j4(a, b, c, d),
    funcao4: function (a, b, c, d) {
      return this.marca + ":" + j4(a, b, c, d);
    },
  };
  return self;
}
const holder = fabricaComArrow();
const arrowIdent4 = holder.arrow4(1, 2, 3, 4);
const arrowExpr4 = id(holder).arrow4(1, 2, 3, 4);
const funcaoIdent4 = holder.funcao4(1, 2, 3, 4);
const funcaoExpr4 = id(holder).funcao4(1, 2, 3, 4);

// Método de CLASSE com 4/5 args sobre receiver-expressão.
class Caixa {
  constructor(n) {
    this.n = n;
  }
  soma4(a, b, c, d) {
    return this.n + a + b + c + d;
  }
  soma5(a, b, c, d, e) {
    return this.n + a + b + c + d + e;
  }
}
function novaCaixa() {
  return new Caixa(100);
}
const classeIdent4 = new Caixa(100).soma4(1, 2, 3, 4);
const classeExpr4 = novaCaixa().soma4(1, 2, 3, 4);
const classeExpr5 = novaCaixa().soma5(1, 2, 3, 4, 5);

// O receptor-expressão só pode ser avaliado UMA vez (efeito colateral).
let vezes = 0;
function contaEChama() {
  vezes = vezes + 1;
  return alvo;
}
const efeito4 = contaEChama().comThis4(1, 2, 3, 4);
const vezesApos = vezes;

describe("método com 4+ argumentos sobre receiver-expressão (#2039)", () => {
  test("receiver-identificador mantém `this` e todas as posições", () => {
    expect(identThis4).toBe("T:1,2,3,4");
    expect(identThis5).toBe("T:1,2,3,4,5");
    expect(identSem4).toBe("P:1,2,3,4");
    expect(identSem5).toBe("P:1,2,3,4,5");
  });

  test("receiver-expressão: chamada de função", () => {
    expect(exprChamada4).toBe("T:1,2,3,4");
    expect(exprChamada5).toBe("T:1,2,3,4,5");
  });

  test("receiver-expressão: callee this-first e callee comum", () => {
    expect(exprId4).toBe("T:1,2,3,4");
    expect(exprIdSem4).toBe("P:1,2,3,4");
    expect(exprIdSem5).toBe("P:1,2,3,4,5");
  });

  test("receiver-expressão: `||`, índice computado, elemento de array", () => {
    expect(exprOu4).toBe("T:1,2,3,4");
    expect(exprIdx4).toBe("T:1,2,3,4");
    expect(exprIdx5).toBe("T:1,2,3,4,5");
    expect(exprArr4).toBe("T:1,2,3,4");
  });

  test("`.apply` com 4 e 5 argumentos", () => {
    expect(applyThis4).toBe("T:1,2,3,4");
    expect(applyThis5).toBe("T:1,2,3,4,5");
    expect(applySem4).toBe("P:1,2,3,4");
    expect(applySem5).toBe("P:1,2,3,4,5");
  });

  test("`.call` com 4 e 5 argumentos", () => {
    expect(callThis4).toBe("T:1,2,3,4");
    expect(callSem4).toBe("P:1,2,3,4");
    expect(callSem5).toBe("P:1,2,3,4,5");
  });

  test("fn this-first invocada sem receptor não perde o 4º argumento", () => {
    expect(semReceptor4).toBe("N:1,2,3,4");
    expect(semReceptorDireto).toBe("N:1,2,3,4");
  });

  test("`...spread` sobre receiver-expressão", () => {
    expect(spreadExpr5).toBe("T:1,2,3,4,5");
    expect(spreadSem5).toBe("P:1,2,3,4,5");
  });

  test("`.bind` parcial não perde os argumentos deslocados", () => {
    expect(bind1de4).toBe("P:1,2,3,4");
    expect(bind2de5).toBe("P:1,2,3,4,5");
  });

  test("campo-arrow mantém o `this` léxico; campo `function` vê o receptor", () => {
    expect(arrowIdent4).toBe("LEX:1,2,3,4");
    expect(arrowExpr4).toBe("LEX:1,2,3,4");
    expect(funcaoIdent4).toBe("RECV:1,2,3,4");
    expect(funcaoExpr4).toBe("RECV:1,2,3,4");
  });

  test("método de classe sobre receiver-expressão", () => {
    expect(classeIdent4).toBe(110);
    expect(classeExpr4).toBe(110);
    expect(classeExpr5).toBe(115);
  });

  test("receptor-expressão é avaliado uma única vez", () => {
    expect(efeito4).toBe("T:1,2,3,4");
    expect(vezesApos).toBe(1);
  });
});
