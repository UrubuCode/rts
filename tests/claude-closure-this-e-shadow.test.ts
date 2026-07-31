import { describe, test, expect } from "rts:test";

// Duas limitações do LIFTING de closures (`funcval::try_extract`) que derrubavam
// bundles reais inteiros. Ambas eram conservadorismo, não capacidade ausente.
//
// 1. `this` sintetizado + capturas disputavam o slot 0. Uma função-expressão que
//    usa `this` E captura algo bailava ("expression arrow"). Isso é EXATAMENTE o
//    `babelHelpers.inheritsLoose` — `function t(){ return e.apply(this,
//    arguments) || this }` — que todo bundle transpilado por Babel emite. O
//    layout agora é `[this, ...captures, ...params]`: `this` fica em params[0]
//    (é assim que `FnSig::has_this` o vê e o invoker liga o receiver em a0) e o
//    thunk lê o env a partir do índice 1.
//
// 2. `mutated_names` confundia SHADOWING com mutação: descia em funções
//    aninhadas coletando todo alvo de atribuição sem respeitar escopo, então um
//    `var r` local numa função irmã marcava o param `r` de fora como mutado.
//    Minificador reusa `r`/`t`/`n` dezenas de vezes por arquivo, então UMA
//    colisão envenenava todas as capturas daquele nome no módulo.
//
// O piso de solidez NÃO mudou: mutação genuína de um param continua bailando
// (os dois últimos testes provam isso — são a contra-prova de que cobertura não
// foi trocada por correção).
//
// Medido nos bundles do WhatsApp Web: as ocorrências de "expression arrow"
// caíram de 19 para 0 num único arquivo de 108 módulos.
//
// Pré-computado no top-level (regra do projeto).

// ── 1. `this` + captura na mesma função ────────────────────────────────────
function envolve(base: any): any {
  function interno(): any { return base + this.x; }
  return interno;
}
const objA: any = { x: 10, m: envolve(5) };
const comThisECaptura = objA.m();

// a forma literal do helper do Babel
function inheritsLoose(pai: any): any {
  function filho(): any { return pai.apply(this, arguments) || this; }
  return filho;
}
const objB: any = { x: 3, m: inheritsLoose(function () { return 42; }) };
const helperBabel = objB.m();

// duas capturas + `this`
function duasCapturas(a: number, b: number): any {
  function t(): any { return a + b + this.z; }
  return t;
}
const objC: any = { z: 100, f: duasCapturas(1, 2) };
const duasMaisThis = objC.f();

// `this` sozinho (não pode regredir)
function soThis(): any {
  function t(): any { return this.v; }
  return t;
}
const objD: any = { v: 7, m: soThis() };
const apenasThis = objD.m();

// ── 2. shadowing numa função irmã não conta como mutação ───────────────────
// `r` é param do escopo externo; o `var r` dentro de `c` é OUTRA variável.
function shadowIrmao(r: any): number {
  function usa(): any { return r(1); }
  function c(): number { var r; r = 1; return r; }
  return usa() + c();
}
const comShadow = shadowIrmao(function (n: number) { return n * 10; });

// captura mutável REAL (vira célula) continua funcionando
function contador(inicio: number): any {
  var n = inicio;
  var passo = function (): number { n = n + 1; return n; };
  return passo;
}
const inc = contador(5);
const primeiro = inc();
const segundo = inc();

describe("closure com `this` e captura juntos", () => {
  test("captura + this.x", () => {
    expect(comThisECaptura).toBe(15);
  });

  test("babelHelpers.inheritsLoose compila e roda", () => {
    expect(helperBabel).toBe(42);
  });

  test("duas capturas mais this", () => {
    expect(duasMaisThis).toBe(103);
  });

  test("this sozinho não regrediu", () => {
    expect(apenasThis).toBe(7);
  });
});

describe("shadowing não é mutação", () => {
  test("`var r` numa função irmã não impede a captura de `r`", () => {
    expect(comShadow).toBe(11);
  });

  test("captura mutável de verdade vira célula e conta", () => {
    expect(primeiro).toBe(6);
    expect(segundo).toBe(7);
  });
});
