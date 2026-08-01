import { describe, test, expect } from "rts:test";

// Ao sintetizar o parâmetro `this` de uma função, a reescrita
// `Raw("This(…)") → Ident("this")` descia TAMBÉM dentro de funções aninhadas —
// inclusive de FUNCTION EXPRESSIONS, que têm `this` PRÓPRIO, ligado por quem as
// chama. Descer ali estava errado duas vezes:
//
//   1. semanticamente, prendia o `this` de dentro ao receptor de FORA;
//   2. como deixava `Ident("this")` onde havia `Raw("This(…)")`, o
//      `body_uses_raw_this` da função interna passava a responder NÃO, ela nunca
//      ganhava o próprio parâmetro `this`, e `this` aparecia como ident livre
//      não-capturável — a função de fora inteira falhava como `expression arrow`.
//
// Medido num bundle real do WhatsApp Web (`ext0.js`, o polyfill de
// `Array.prototype.values`); forma mínima:
//
//   (function(){ var t = {}; t.next = function(){ this.a++; }; })(this)
//
// O argumento `(this)` é o que arrastava o topo para uma reescrita de `this`; o
// mesmo arquivo com `(0)` compilava. A reescrita agora PARA na fronteira de uma
// function expression e continua descendo em ARROW (que herda `this` léxico).
//
// Valores conferidos contra o Node. Pré-computado no top-level.

// ── 1. A forma do bundle: `this` de dentro é o do CHAMADOR ───────────────────
// Antes: `expression arrow`. Node: "6 6".
function fnExprTemProprioThis() {
  var t: any = {};
  t.next = function (this: any) { this.a++; return this.a; };
  const obj: any = { a: 5, next: t.next };
  return "" + obj.next() + " " + obj.a;
}

// ── 2. E não é o `this` de fora, mesmo quando a de fora tem um ───────────────
// Node: "outer inner".
function naoVazaDeFora(this: any) {
  const marca = this.tag;
  var t: any = {};
  t.next = function (this: any) { return this.tag; };
  const obj: any = { tag: "inner", next: t.next };
  return "" + marca + " " + obj.next();
}

// A forma "arrow lendo `this` dentro de uma FUNÇÃO LIVRE com parâmetro `this`
// declarado" (`function f(this: any){ const g = () => this.tag; return g(); }`,
// chamada por `f.call({tag:"outer"})`) ficou de fora: ela já falhava ANTES desta
// mudança, com `expression raw/unrecognized: This(…)`, e continua falhando —
// nada aqui a toca. O caso 4 abaixo cobre "arrow herda o `this` léxico" pelo
// caminho que funciona (dentro de método de classe).

// ── 3. Método de classe com function expression aninhada ─────────────────────
// Node: "10 99" — o `this` da fn-expr é o receptor dela, não a instância.
class Caixa {
  v: number;
  constructor(v: number) { this.v = v; }
  fazer(): string {
    const meu = this.v;
    const g = function (this: any) { return this.v; };
    const outro: any = { v: 99, g: g };
    return "" + meu + " " + outro.g();
  }
}

// ── 4. Arrow DENTRO de method + fn-expr irmã: cada uma com seu `this` ────────
// Node: "7 42".
class Mista {
  v: number;
  constructor(v: number) { this.v = v; }
  fazer(): string {
    const a = (() => this.v)();
    const f = function (this: any) { return this.v; };
    const alvo: any = { v: 42, f: f };
    return "" + a + " " + alvo.f();
  }
}

const r1 = fnExprTemProprioThis();
const r2 = naoVazaDeFora.call({ tag: "outer" });
const r3 = new Caixa(10).fazer();
const r4 = new Mista(7).fazer();

describe("`this` de function expression aninhada", () => {
  test("a fn-expr aninhada tem `this` próprio (forma do bundle real)", () => {
    expect(r1).toBe("6 6");
  });
  test("o `this` de fora não vaza para dentro da fn-expr", () => {
    expect(r2).toBe("outer inner");
  });
  test("fn-expr dentro de método de classe usa o receptor dela", () => {
    expect(r3).toBe("10 99");
  });
  test("arrow e fn-expr irmãs num método, cada uma com seu `this`", () => {
    expect(r4).toBe("7 42");
  });
});
