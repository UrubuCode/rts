import { describe, test, expect } from "rts:test";
import { time } from "rts";

// O pré-passo que adapta o JS de página ao subset do motor foi movido de `.ts`
// para RUST (`scriptscan.rs`). Este teste é o CONTRATO dessa troca: o scanner
// rápido tem de produzir exatamente o que a versão `.ts` produzia.
//
// Por que a troca: varrer o fonte caractere a caractere em `.ts` custava ~1 µs
// por char. Num bundle real da Meta (6 MB) isso dava **49 s de pré-passo**
// contra **113 ms de compilação de verdade** — parecia "bundle travando o
// carregamento", e era só varredura quadrática do nosso lado. Em Rust o mesmo
// bundle leva ~0,4 s.
//
// As funções `__normalizeScriptTs` / `__scanImplicitGlobalsTs` continuam no
// `scriptscope.ts` justamente para servir de ORÁCULO aqui — se alguém mudar um
// lado sem o outro, este teste quebra.
//
// Pré-computado no top-level (regra do projeto).

// ── normalização: os dois lados têm de gerar o MESMO texto ─────────────────
const casos: string[] = [
  "a=1,b=2",                                    // sequência de topo → `;`
  "f(1,2),g(3,4)",                              // vírgula de ARGUMENTO fica
  "var a=[1,2],b={x:1,y:2};c=1,d=2",            // array/objeto ficam
  "var s=\"x,y\";c=1,d=2",                      // vírgula em string fica
  "/* bloco, aqui */z=1,w=2",                   // vírgula em comentário fica
  "var re=/a,b/;p=1,q=2",                       // vírgula em regex fica
  "var tpl=`t,${1+1},u`;m=1,n=2",               // vírgula em template fica
  "x instanceof window.Foo",                    // `window.` some
  "var t='a instanceof window.Bar';y instanceof self.Baz", // string intacta
  "var f=function(){return arguments.length}",  // arguments → rest
  "var g=function(a){return arguments.length}", // com param: NÃO mexe
  "for(var i=0,j=1;i<2;i++){}",                 // vírgula dentro de `for(...)`
];

let normDif = 0;
let i = 0;
while (i < casos.length) {
  if (__normalizeScriptTs(casos[i]) !== __normalizeScript(casos[i])) normDif = normDif + 1;
  i = i + 1;
}

// ── globais implícitos: mesmo CONJUNTO (após filtrar declarados) ───────────
const casosG: string[] = [
  "requireLazy = function(){};",
  "var a = 1; b = 2;",
  "x=1,y=2;",
  "function f(){ g = 1 }",
  "obj.prop = 1;",           // campo não é global
  "if (a == 1) {} c = 3;",   // `==` não é atribuição
  "class C { nome = 1 } d = 4;", // campo de classe não é global
  "var s = \"z = 1\"; w = 2;",   // atribuição em string não conta
];

let gDif = 0;
let k = 0;
while (k < casosG.length) {
  const norm = __normalizeScript(casosG[k]);
  const ts = __filterDeclared(__scanImplicitGlobalsTs(norm), norm);
  const rs = __scanImplicitGlobals(norm);
  if (ts.length !== rs.length) {
    gDif = gDif + 1;
  } else {
    let j = 0;
    while (j < ts.length) {
      let achou = 0;
      let m = 0;
      while (m < rs.length) { if (rs[m] === ts[j]) achou = 1; m = m + 1; }
      if (achou === 0) gDif = gDif + 1;
      j = j + 1;
    }
  }
  k = k + 1;
}

// ── o caso que motivou tudo: um script grande não pode custar segundos ─────
// 200 KB de código sintético (a ordem de grandeza de um bundle de aplicação).
let grande = "";
let g = 0;
while (g < 4000) {
  grande = grande + "function f" + g + "(a,b){var t=a+b;return t}\n";
  g = g + 1;
}
const tamGrande = grande.length;
const t0 = time.now_ms();
const normGrande = __normalizeScript(grande);
const msNorm = time.now_ms() - t0;
const t1 = time.now_ms();
__scanImplicitGlobals(normGrande);
const msScan = time.now_ms() - t1;

describe("ScriptScan (Rust) tem paridade com o oráculo .ts", () => {
  test("normalização gera texto idêntico", () => {
    expect(normDif).toBe(0);
  });

  test("globais implícitos dão o mesmo conjunto", () => {
    expect(gDif).toBe(0);
  });
});

describe("ScriptScan — custo", () => {
  test("pré-passo de ~200 KB não é O(n²)", () => {
    // Em `.ts` isto levava ~1,7 s (normalize+scan); em Rust ~30 ms. O teto de
    // 3 s é folgado de propósito: pega a regressão de ORDEM DE GRANDEZA (voltar
    // a varrer em `.ts`) sem flakear em máquina lenta ou build de debug.
    expect(tamGrande > 100000).toBe(true);
    expect(msNorm < 3000).toBe(true);
    expect(msScan < 3000).toBe(true);
  });
});
