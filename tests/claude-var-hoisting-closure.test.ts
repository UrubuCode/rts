import { describe, test, expect } from "rts:test";

// `var` HOISTING e DESTRUCTURING dentro de um corpo de função INLINE
// (`xs.map(function (o) { … })`, `() => { … }`, `function inner(){…}` aninhada).
//
// O bug: `rts-ast` só carrega os statements do MÓDULO e cada `function`
// top-level, então um corpo escrito inline não tinha contraparte swc e nunca era
// visitado pelos passes que precisam do par (swc, HIR). Dois defeitos saíam daí,
// os dois em cima de formas que todo bundle minificado usa:
//
//   1. `var` não era hoistado lá dentro — uma atribuição textualmente ANTES do
//      `var` (`R = k + 2; var R;`) baila "assignment to unbound `R`";
//   2. um binding por destructuring (`var { R } = o`) mantinha o nome achatado
//      `"_"` que o rts-hir produz, então `R` nunca era ligado ("R is not
//      defined" em runtime).
//
// Terceiro defeito, da mesma família e achado no caminho: o pareamento
// swc↔HIR do passe de destructuring é POSICIONAL, e o prólogo sintético que o
// hoist de `var` (e o objeto `arguments`) prepende ao corpo deslocava todos os
// índices — `var a = 1; var { R } = o;` na MESMA função deixava de expandir.
//
// Valores conferidos um a um contra o Node.

// ── 1. atribuição antes do `var`, dentro de callback ──────────────────────
const xs1 = [1];
const antesDoVar = xs1.map(function (k) { R = k + 2; var R; return R; })[0];
const antesDoVarComInit = xs1.map(function (k) { R = k + 2; var R = 0; return R; })[0];
const antesDoVarMisto = xs1.map(function (k) {
  var Q = 0;
  R = k + 2;
  var R = 1;
  return R + Q;
})[0];
// mesma coisa numa arrow inline (não é `function`)
const antesDoVarArrow = xs1.map((k) => { R = k + 5; var R; return R; })[0];

// ── 2. destructuring de objeto e de array, leitura e atribuição ───────────
const xs2 = [{ R: 1 }];
const destrObjEscrita = xs2.map(function (o) { var { R } = o; R = R + 1; return R; })[0];
const destrObjLeitura = xs2.map(function (o) { var { R } = o; return R; })[0];
const xs3 = [[1]];
const destrArr = xs3.map(function (o) { var [R] = o; R = R + 1; return R; })[0];

// ── 3. destructuring aninhado, com default, com rest ─────────────────────
const destrAninhado = [{ a: { b: 2 } }].map(function (o) { var { a: { b } } = o; return b + 1; })[0];
const destrDefault = [{}].map(function (o) { var { z = 9 } = o; return z; })[0];
const destrRest = [[1, 2, 3]].map(function (o) { var [h, ...t] = o; return h + t.length; })[0];

// ── 4. `var` dentro de bloco/loop/try escapa o bloco (escopo de FUNÇÃO) ───
const varEmBloco = [1].map(function (k) { if (k) { var R = 9; } R = k + 2; return R; })[0];
const varEmFor = [1].map(function (k) { for (var i = 0; i < k; i++) { } return i; })[0];
const varEmTry = [1].map(function (k) {
  try { var R = k; } catch (e) { R = 2; }
  return R + 1;
})[0];

// ── 5. `function` DECLARADA dentro de outra função ────────────────────────
function comInner(o: any): number {
  function inner(p: any): number { var { R } = p; return R; }
  return inner(o);
}
const innerDestr = comInner({ R: 7 });

// ── 6. prólogo do hoist não pode desalinhar o par swc↔HIR ─────────────────
function varMaisDestr(o: any): number { var a = 1; var { R } = o; return R + a; }
const misturaEmFn = varMaisDestr({ R: 7 });

// mesmo caso em escopo de MÓDULO (o prólogo vai pro corpo do `__rts_startup`)
var oTop = { R: 7 };
var aTop = 1;
var { R: rTop } = oTop;
const misturaNoTopo = rTop + aTop;

// ── 7. casos que JÁ funcionavam — travas anti-regressão ───────────────────
const varAntesDeUsar = xs1.map(function (k) { var R; R = k + 2; return R; })[0];
const letAntesDeUsar = xs1.map(function (k) { let R; R = k + 2; return R; })[0];
const varComInit = xs1.map(function (k) { var R = k; R = R + 1; return R; })[0];
const constDestr = [{ R: 4 }].map(function (o) { const { R } = o; return R; })[0];
function topLevelVarDepois(k: number): number { R = k + 2; var R = 0; return R; }
const emFnTopLevel = topLevelVarDepois(1);
// captura mutável de um `var` do escopo externo (não é hoisting local)
function capturaExterna(): number {
  var R = 1;
  const g = () => { R = R + 1; return R; };
  return g();
}
const capturada = capturaExterna();

describe("var hoisting + destructuring em corpo de função inline", () => {
  test("atribuição antes do `var` no callback", () => {
    expect(antesDoVar).toBe(3);
  });

  test("atribuição antes do `var` com inicializador", () => {
    expect(antesDoVarComInit).toBe(0);
  });

  test("`var` hoistado convive com outro `var` já declarado", () => {
    expect(antesDoVarMisto).toBe(1);
  });

  test("mesma forma numa arrow inline", () => {
    expect(antesDoVarArrow).toBe(6);
  });

  test("destructuring de objeto — leitura", () => {
    expect(destrObjLeitura).toBe(1);
  });

  test("destructuring de objeto — leitura e atribuição", () => {
    expect(destrObjEscrita).toBe(2);
  });

  test("destructuring de array", () => {
    expect(destrArr).toBe(2);
  });

  test("destructuring aninhado", () => {
    expect(destrAninhado).toBe(3);
  });

  test("destructuring com default", () => {
    expect(destrDefault).toBe(9);
  });

  test("destructuring com rest", () => {
    expect(destrRest).toBe(3);
  });

  test("`var` declarado dentro de bloco escapa o bloco", () => {
    expect(varEmBloco).toBe(3);
  });

  test("`var` do cabeçalho do for sobrevive ao loop", () => {
    expect(varEmFor).toBe(1);
  });

  test("`var` declarado no try é visível depois", () => {
    expect(varEmTry).toBe(2);
  });

  test("`function` aninhada com destructuring", () => {
    expect(innerDestr).toBe(7);
  });

  test("`var` simples + destructuring na mesma função", () => {
    expect(misturaEmFn).toBe(8);
  });

  test("`var` simples + destructuring no topo do módulo", () => {
    expect(misturaNoTopo).toBe(8);
  });
});

describe("var hoisting — travas anti-regressão", () => {
  test("`var` declarado antes do uso", () => {
    expect(varAntesDeUsar).toBe(3);
  });

  test("`let` declarado antes do uso", () => {
    expect(letAntesDeUsar).toBe(3);
  });

  test("`var` com inicializador e reatribuição", () => {
    expect(varComInit).toBe(2);
  });

  test("destructuring com `const` no callback", () => {
    expect(constDestr).toBe(4);
  });

  test("hoisting em função top-level segue igual", () => {
    expect(emFnTopLevel).toBe(0);
  });

  test("captura mutável de `var` externo", () => {
    expect(capturada).toBe(2);
  });
});
