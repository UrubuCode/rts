import { describe, test, expect } from "rts:test";

// Um PARÂMETRO capturado por closure E reatribuído vira uma CÉLULA, igual a um
// `let` na mesma situação. Antes o lifter só aceitava encaixotar um `let`, e um
// param mutado o fazia RECUSAR a extração — o arquivo inteiro morria em
// "expression arrow".
//
// A construção que isso serve é o PARÂMETRO COM VALOR PADRÃO transpilado:
//   function s(a, flag = false)
// vira
//   function s(a, flag) { flag === void 0 && (flag = false); … }
// ou seja, o param é reatribuído — então toda closure que o captura ficava sem
// caminho sólido. Em bundle transpilado isso é onipresente.
//
// A cell é alocada no PRÓLOGO da própria função, a partir do argumento
// recebido: é a mesma regra do `let` (quem declara, aloca). Uma exclusão é
// obrigatória: um param de CAPTURA de closure sintetizada já chega com o
// HANDLE, e re-alocar encaixotaria a caixa (medido: a closure devolvia `[15]`
// onde o Node dá `15`, e escritas pousavam na caixa errada).
//
// Valores conferidos contra Node e Bun (fixture cross-runtime
// tests/cross-runtime/fn-meta/424_param_captured_and_reassigned.ts).

// ── o padrão do parâmetro com default transpilado ───────────────────────────
function comDefault(a: any, flag: any): any {
  flag === undefined && (flag = false);
  const ler = function (): any { return "flag=" + flag; };
  return ler();
}
const defOmitido = comDefault(1, undefined);
const defDado = comDefault(1, true);

// ── o param muda DEPOIS da closure existir ──────────────────────────────────
function mudaDepois(n: any): any {
  const ler = function (): any { return n; };
  n = n + 10;
  return ler();
}
const depois = mudaDepois(5);

// ── a closure ESCREVE o param; o corpo externo enxerga ──────────────────────
function closureEscreve(v: any): any {
  const set = function (x: any): void { v = x; };
  set("novo");
  return v;
}
const escrito = closureEscreve("velho");

// ── duas closures compartilham o MESMO param ────────────────────────────────
function duasClosures(p: any): any {
  const inc = function (): void { p = p + 1; };
  const ler = function (): any { return p; };
  inc();
  inc();
  return ler();
}
const duas = duasClosures(0);

// ── cada CHAMADA tem sua própria caixa ──────────────────────────────────────
function contador(inicio: any): any {
  return function (): any { inicio = inicio + 1; return inicio; };
}
const c1 = contador(0);
const c2 = contador(100);
const c1a = c1();
const c1b = c1();
const c2a = c2();

describe("param capturado e reatribuído vira célula", () => {
  test("parâmetro com default transpilado", () => {
    expect(defOmitido).toBe("flag=false");
    expect(defDado).toBe("flag=true");
  });
  test("closure vê a mutação feita depois de criada", () => {
    expect(depois).toBe(15);
  });
  test("escrita da closure alcança o corpo externo", () => {
    expect(escrito).toBe("novo");
  });
  test("duas closures compartilham a mesma caixa", () => {
    expect(duas).toBe(2);
  });
  test("cada chamada tem sua própria caixa", () => {
    expect(c1a).toBe(1);
    expect(c1b).toBe(2);
    expect(c2a).toBe(101);
  });
});
