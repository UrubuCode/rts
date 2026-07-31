import { describe, test, expect } from "rts:test";

// `values()`/`keys()`/`entries()` de Array, Map e Set passam a suportar o
// protocolo `.next()`. Antes falhavam em COMPILAÇÃO — `no Registry entry for
// Array.next(0 args)` —, porque o resultado é um array materializado e `Array`
// não tem linha de Registry para `next`.
//
// Duas peças:
//   1. o handle devolvido é REGISTRADO como iterador aberto na criação
//      (`open_vec_iterator`), o que o torna distinguível de um array comum em
//      runtime sem side-table nova — `GEN_CURSORS` já existia para o generator
//      eager, e `generator_next` já cursoriza `Entry::Vec`;
//   2. `.next()`/`.return()`/`.throw()` sobre receiver-array roteia ao despacho
//      dinâmico, que aceita `Entry::Vec` SOMENTE se marcado.
//
// A distinção tem uma armadilha que já derrubou uma tentativa anterior:
// `generator_next` usa `or_insert(0)`, ou seja, CRIA o cursor para qualquer
// handle que receba. Perguntar "tem cursor?" depois responde sim para um array
// comum também. Por isso o predicado é `contains_key` puro, nunca inserindo —
// assim continua sendo uma propriedade de COMO o handle foi criado, e
// `[1,2].next` segue `undefined`.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

const arrIt = [1, 2].values();
const primeiro = arrIt.next().value;
const segundo = arrIt.next().value;
const esgotado = JSON.stringify(arrIt.next());

const keysPrimeiro = [7, 8].keys().next().value;
const entriesPrimeiro = JSON.stringify([9].entries().next().value);

const s = new Set([5, 6]);
const setPrimeiro = s.values().next().value;
const setKeys = s.keys().next().value;

const m = new Map([["k", 9]]);
const mapEntries = JSON.stringify(m.entries().next().value);
const mapKeys = m.keys().next().value;
const mapValues = m.values().next().value;

// o cursor é do ITERADOR, não do array de origem: dois iteradores sobre o mesmo
// array andam independentes
const base = [10, 20];
const itA = base.values();
const itB = base.values();
itA.next();
const independentes = itB.next().value;

// `arr[Symbol.iterator]()` — a CHAMADA (a leitura já devolvia uma função).
const simIt = [1, 2, 3][Symbol.iterator]();
const simTypeof = typeof simIt;
const simPrimeiro = simIt.next().value;
const simSegundo = simIt.next().value;
const simSpread = [...[4, 5][Symbol.iterator]()].join(",");

// `take` sobre iterador — a forma que os bundles usam
function take(iter, n) {
  const out: any[] = [];
  for (let i = 0; i < n; i++) {
    const r = iter.next();
    if (r.done) break;
    out.push(r.value);
  }
  return out;
}
const viaTake = take([1, 2, 3, 4].values(), 3).join(",");

// ── não-regressões ─────────────────────────────────────────────────────────
const arrayComumNext = ([1, 2] as any).next;
const forOfValues = [...[1, 2].values()].join(",");
const forOfKeys = [...[7, 8].keys()].join(",");
const joinDeValues = [1, 2, 3].values().join("-");
const lengthDeValues = [1, 2, 3].values().length;

let somaForOf = 0;
for (const v of [1, 2, 3].values()) { somaForOf = somaForOf + v; }

let paresOk = "";
for (const [i, v] of [9, 8].entries()) { paresOk = paresOk + i + ":" + v + " "; }

describe("protocolo .next() em iteradores nativos", () => {
  test("array values() rende o primeiro valor", () => {
    expect(primeiro).toBe(1);
  });

  test("array values() avança o cursor", () => {
    expect(segundo).toBe(2);
  });

  test("array values() esgota com done", () => {
    expect(esgotado).toBe('{"done":true}');
  });

  test("array keys()", () => {
    expect(keysPrimeiro).toBe(0);
  });

  test("array entries()", () => {
    expect(entriesPrimeiro).toBe("[0,9]");
  });

  test("Set values()", () => {
    expect(setPrimeiro).toBe(5);
  });

  test("Set keys()", () => {
    expect(setKeys).toBe(5);
  });

  test("Map entries()", () => {
    expect(mapEntries).toBe('["k",9]');
  });

  test("Map keys()", () => {
    expect(mapKeys).toBe("k");
  });

  test("Map values()", () => {
    expect(mapValues).toBe(9);
  });

  test("dois iteradores do mesmo array são independentes", () => {
    expect(independentes).toBe(10);
  });

  test("take() sobre iterador — a forma dos bundles", () => {
    expect(viaTake).toBe("1,2,3");
  });

  test("arr[Symbol.iterator]() devolve um objeto, não um número", () => {
    expect(simTypeof).toBe("object");
  });

  test("arr[Symbol.iterator]().next() rende o primeiro valor", () => {
    expect(simPrimeiro).toBe(1);
  });

  test("arr[Symbol.iterator]() avança o cursor", () => {
    expect(simSegundo).toBe(2);
  });

  test("spread de arr[Symbol.iterator]()", () => {
    expect(simSpread).toBe("4,5");
  });
});

describe("não-regressões: array comum e consumo por iterável", () => {
  test("array comum NÃO responde a .next", () => {
    expect(arrayComumNext).toBe(undefined);
  });

  test("spread de values() não regrediu", () => {
    expect(forOfValues).toBe("1,2");
  });

  test("spread de keys() não regrediu", () => {
    expect(forOfKeys).toBe("0,1");
  });

  test("for-of de values() não regrediu", () => {
    expect(somaForOf).toBe(6);
  });

  test("destructuring de entries() em for-of não regrediu", () => {
    expect(paresOk).toBe("0:9 1:8 ");
  });

  test("values() continua respondendo a métodos de array (.join)", () => {
    expect(joinDeValues).toBe("1-2-3");
  });

  test("values() continua respondendo a .length", () => {
    expect(lengthDeValues).toBe(3);
  });
});
