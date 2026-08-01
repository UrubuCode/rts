import { describe, test, expect } from "rts:test";

// `yield*` sobre Set / Map / iterável custom — produziam `[]` em silêncio.
//
// A normalização da fonte de um `yield*` vive em `rts-natives`, que só sabe
// decodificar `GenState`, `Vec` e `String`. Um `Set`, um `Map` ou um objeto com
// `[Symbol.iterator]` é valor do modelo da camada de CIMA, então a delegação
// reportava "esgotado" no primeiro passo:
//
//   function* g(){ yield* new Set([1,2,3]); }
//   [...g()]        // RTS: []        ·  Node: [1,2,3]
//
// Resposta errada em silêncio, não erro. Pré-existente (medido no binário
// anterior a esta frente).
//
// A armadilha que quase fez o conserto errado: o discriminador NÃO pode ser
// "não é um Vec". Um `Set`/`Map`/instância de classe É um `Entry::Vec` — o
// objeto com shape guarda os slots num Vec — então a checagem por tipo dava
// "Vec" e a delegação iterava os SLOTS DO OBJETO. Instrumentar mostrou
// `kind=2` onde eu esperava `kind=0`. Quem sabe distinguir array de
// objeto-com-shape é o value model, e é ele que responde por `iter_open`.
//
// Correção: só `GenState` continua nativo (a delegação lazy dele precisa do
// `sent` e do `ret`, que são campos daquele crate); todo o resto vai para o
// protocolo `__rtsadp_iter_*` — o MESMO que o for-of comum usa — através de uma
// ponte de fn-pointers instalada no `runtime_init`, na mesma costura do
// `AgenDriver`. O cursor é aberto LAZY e andado um valor por vez: materializar
// a fonte penduraria num iterador custom infinito, que é troca pior que o
// caminho vazio que ela substitui.
//
// Todos os valores conferidos contra o Node.

function* dSet() { yield* new Set([1, 2, 3]); }
const sobreSet = JSON.stringify([...dSet()]);

function* dMap() { yield* new Map<string, number>([["k", 1], ["j", 2]]); }
const sobreMap = JSON.stringify([...dMap()]);

class Range {
  lo: number;
  hi: number;
  constructor(lo: number, hi: number) { this.lo = lo; this.hi = hi; }
  *[Symbol.iterator]() { let i = this.lo; while (i < this.hi) { yield i; i = i + 1; } }
}
function* dCustom() { yield* new Range(4, 7); }
const sobreIteravelCustom = JSON.stringify([...dCustom()]);

function* dEmpty() { yield* new Set<number>([]); yield "depois"; }
const fonteVazia = JSON.stringify([...dEmpty()]);

// Set em posição de VALOR: a delegação não tem `return`, então `undefined`.
function* dVal() { const r = yield* new Set([9]); return "r=" + r; }
const v1: any = dVal();
const setEmPosicaoDeValor = JSON.stringify([v1.next(), v1.next()]);

// ── controles: as fontes que já funcionavam, byte a byte ──────────────────
function* src() { yield "a"; return "R"; }
function* cGen() { const r = yield* src(); return "got:" + r; }
const c1: any = cGen();
const controleGenerator = JSON.stringify([c1.next(), c1.next()]);

function* cArr() { yield* [1, 2]; yield 3; }
const controleArray = JSON.stringify([...cArr()]);

function* cStr() { yield* "ab"; }
const controleString = JSON.stringify([...cStr()]);

describe("generator: yield* sobre Set/Map/iteravel custom", () => {
  test("Set", () => expect(sobreSet).toBe("[1,2,3]"));
  test("Map", () => expect(sobreMap).toBe('[["k",1],["j",2]]'));
  test("iteravel custom via Symbol.iterator", () =>
    expect(sobreIteravelCustom).toBe("[4,5,6]"));
  test("fonte vazia nao engole o que vem depois", () =>
    expect(fonteVazia).toBe('["depois"]'));
  test("Set em posicao de valor", () =>
    expect(setEmPosicaoDeValor).toBe(
      '[{"value":9,"done":false},{"value":"r=undefined","done":true}]',
    ));
  test("controle: delegar generator preserva o return", () =>
    expect(controleGenerator).toBe(
      '[{"value":"a","done":false},{"value":"got:R","done":true}]',
    ));
  test("controle: delegar array", () => expect(controleArray).toBe("[1,2,3]"));
  test("controle: delegar string", () => expect(controleString).toBe('["a","b"]'));
});
