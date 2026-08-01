import { describe, test, expect } from "rts:test";

// (bundle real) `for…of`, `for…in`, `do…while` e `switch` DENTRO de generator.
//
// A state-machine do generator modelava `while`, `for(;;)`, `if`, `try`,
// `break`/`continue` e `return` — e devolvia `None` para todo o resto. `None`
// aborta o build e cai no eager-buffer, que só sabe expressar `yield` em posição
// de STATEMENT (vira `__gen_buf.push`). Um `yield` de VALOR (`const a = yield x`,
// o valor que um `.next(v)` posterior manda de volta) sobrevive ao desugar e
// chega cru ao lowering:
//
//   function* g(src){ for (const x of src) { const a = yield x; } }
//   // → "expression raw/unrecognized: Yield(...)"
//
// Era exatamente esse o cluster medido numa carga real (7 de 9 erros da página).
// Agora as quatro formas viram ESTADOS: for-of/for-in andam pelo MESMO protocolo
// lazy (`__rtsadp_iter_*`) que o for-of comum usa, então array/string/Set/Map/
// iterador custom/OUTRO GENERATOR iteram igual dentro e fora de um generator.
//
// Todos os valores conferidos contra o Node (`node tests/...`). Pré-computado
// no top-level (chamar método de instância dentro do `test()` pode pegar GC).

// ── for-of + yield de valor ────────────────────────────────────────────────
function* overOf(src: any) {
  for (const x of src) {
    const a = yield x;
    if (a) yield "+" + a;
  }
  return "end";
}
const of1: any = overOf([1, 2]);
const ofSeq = JSON.stringify([of1.next(), of1.next("A"), of1.next(), of1.next(), of1.next()]);

// break dentro do for-of (roda IteratorClose e sai)
function* ofBreak(src: any) {
  for (const x of src) {
    if (x === 3) break;
    yield x;
  }
  return "done";
}
const ofBreakSeq = [...ofBreak([1, 2, 3, 4])].join(",");

// continue dentro do for-of
function* ofCont(src: any) {
  for (const x of src) {
    if (x % 2 === 0) continue;
    yield x;
  }
}
const ofContSeq = [...ofCont([1, 2, 3, 4, 5])].join(",");

// fontes não-array: string, Map, Set, classe iterável e OUTRO generator
function* plain(src: any) { for (const x of src) yield x; }
function* src3() { let i = 0; while (i < 3) { yield i; i = i + 1; } }
const ofStr = [...plain("abc")].join(",");
const ofMap = JSON.stringify([...plain(new Map<string, number>([["a", 1], ["b", 2]]))]);
const ofSet = [...plain(new Set<number>([7, 8]))].join(",");
const ofGen = [...plain(src3())].join(",");

class Range {
  lo: number;
  hi: number;
  constructor(lo: number, hi: number) { this.lo = lo; this.hi = hi; }
  *[Symbol.iterator]() { let i = this.lo; while (i < this.hi) { yield i; i = i + 1; } }
}
const ofClass = [...plain(new Range(4, 7))].join(",");

// ── for-in + yield de valor ────────────────────────────────────────────────
function* keysOf(o: any) {
  for (const k in o) {
    const a = yield k;
    if (a) yield k + ":" + a;
  }
}
const in1: any = keysOf({ p: 1, q: 2 });
const inSeq = JSON.stringify([in1.next(), in1.next("v"), in1.next(), in1.next()]);

// ── do…while ───────────────────────────────────────────────────────────────
function* doGen(n: number) {
  let i = 0;
  do {
    const a = yield i;
    i = i + 1;
    if (a === "x") break;
  } while (i < n);
  return i;
}
const do1: any = doGen(3);
const doSeq = JSON.stringify([do1.next(), do1.next(), do1.next(), do1.next()]);

// ── switch: fallthrough, default no meio, break, continue do laço externo ──
function* fall(k: number) {
  switch (k) {
    case 1: yield "one";
    case 2: yield "two"; break;
    case 3: yield "three";
  }
  yield "end";
}
const fall1 = [...fall(1)].join(",");
const fall2 = [...fall(2)].join(",");
const fall3 = [...fall(3)].join(",");
const fall9 = [...fall(9)].join(",");

// `continue` DENTRO de um switch pertence ao laço externo, não ao switch.
function* contInSwitch(src: any) {
  for (const x of src) {
    switch (x % 2) {
      case 0: continue;
      default: break;
    }
    yield x;
  }
}
const contSeq = [...contInSwitch([1, 2, 3, 4, 5])].join(",");

// switch com yield de VALOR dentro de um laço, default no fim
function* swLoop() {
  let i = 0;
  while (i < 4) {
    switch (i) {
      case 0: { const a = yield "zero"; if (a) yield "a=" + a; break; }
      case 1:
      case 2: { yield "two-ish:" + i; break; }
      default: { yield "def:" + i; }
    }
    i = i + 1;
  }
  return "fim";
}
const sw1: any = swLoop();
const swSeq = JSON.stringify([
  sw1.next(), sw1.next("X"), sw1.next(), sw1.next(), sw1.next(), sw1.next(),
]);

// ── `next()` SEM argumento manda `undefined`, não o valor anterior ─────────
// O `sent` sobrevivia à retomada: um `next(v)` seguido de `next()` fazia o
// `const a = yield x` seguinte ler o valor VELHO (`{"value":"s:v"}` onde o Node
// dá `{"value":2}`). Agora o `sent` expira ao fim de cada passo.
function* sentDecay() {
  let i = 0;
  while (i < 3) {
    const a = yield i;
    if (a) yield "s:" + a;
    i = i + 1;
  }
}
const st: any = sentDecay();
const sentSeq = JSON.stringify([st.next(), st.next("v"), st.next(), st.next()]);

describe("generator: for-of / for-in / do-while / switch", () => {
  test("for-of com yield de valor", () =>
    expect(ofSeq).toBe(
      '[{"value":1,"done":false},{"value":"+A","done":false},{"value":2,"done":false},{"value":"end","done":true},{"done":true}]',
    ));
  test("break no for-of", () => expect(ofBreakSeq).toBe("1,2"));
  test("continue no for-of", () => expect(ofContSeq).toBe("1,3,5"));
  test("for-of sobre string", () => expect(ofStr).toBe("a,b,c"));
  test("for-of sobre Map", () => expect(ofMap).toBe('[["a",1],["b",2]]'));
  test("for-of sobre Set", () => expect(ofSet).toBe("7,8"));
  test("for-of sobre outro generator", () => expect(ofGen).toBe("0,1,2"));
  test("for-of sobre classe iteravel", () => expect(ofClass).toBe("4,5,6"));
  test("for-in com yield de valor", () =>
    expect(inSeq).toBe(
      '[{"value":"p","done":false},{"value":"p:v","done":false},{"value":"q","done":false},{"done":true}]',
    ));
  test("do-while com yield de valor", () =>
    expect(doSeq).toBe(
      '[{"value":0,"done":false},{"value":1,"done":false},{"value":2,"done":false},{"value":3,"done":true}]',
    ));
  test("switch fallthrough", () => expect(fall1).toBe("one,two,end"));
  test("switch case direto", () => expect(fall2).toBe("two,end"));
  test("switch ultimo case cai fora", () => expect(fall3).toBe("three,end"));
  test("switch sem match", () => expect(fall9).toBe("end"));
  test("continue dentro de switch pertence ao laco", () => expect(contSeq).toBe("1,3,5"));
  test("switch com yield de valor num laco", () =>
    expect(swSeq).toBe(
      '[{"value":"zero","done":false},{"value":"a=X","done":false},{"value":"two-ish:1","done":false},{"value":"two-ish:2","done":false},{"value":"def:3","done":false},{"value":"fim","done":true}]',
    ));
  test("next() sem argumento manda undefined", () =>
    expect(sentSeq).toBe(
      '[{"value":0,"done":false},{"value":"s:v","done":false},{"value":1,"done":false},{"value":2,"done":false}]',
    ));
});
