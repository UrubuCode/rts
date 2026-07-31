import { describe, test, expect } from "rts:test";

// `if` contendo `yield` era INELEGÍVEL para a state-machine, então caía no
// eager-buffer — onde `yield` de VALOR (`const a = yield x`) é reescrito para
// `push(...)`. O resultado era VALOR ERRADO em silêncio:
//
//   function* a(){ if(true){ const v = yield 1; yield v; } }
//   a().next(); a().next(5)        // RTS: 1,undefined  ·  Node: 1,5
//
// A state-machine já tinha o caminho ramificado por estados — usava-o só quando
// havia `return` dentro do `if`. Passa a usá-lo também quando há `yield`:
// `set_cond(test, then, else)` + lower recursivo de cada ramo + `goto after`.
//
// LIMITE mantido: `yield` no TESTE (`if (yield x)`) continua inelegível —
// exigiria suspender NO MEIO da avaliação da condição, que a SM não modela.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

function* comBloco() { if (true) { const v = yield 1; yield v; } }
const ia = comBloco();
const blocoPrimeiro = ia.next().value;
const blocoEnviado = ia.next(5).value;

function* ramoThen() { const v = yield 1; if (v > 0) yield v * 2; else yield 0; }
const ib = ramoThen();
const thenPrimeiro = ib.next().value;
const thenEnviado = ib.next(5).value;

function* ramoElse() { const v = yield 1; if (v > 0) yield v * 2; else yield -1; }
const ic = ramoElse();
ic.next();
const peloElse = ic.next(-3).value;

function* ifAninhado() {
  const x = yield 1;
  if (x > 10) { const y = yield x; yield y + 1; } else { yield 0; }
}
const ig = ifAninhado();
const an1 = ig.next().value;
const an2 = ig.next(20).value;
const an3 = ig.next(7).value;

function* ifEmLoop() { let i = 0; while (i < 3) { if (i % 2 === 0) yield i; i = i + 1; } }
const noLoop = [...ifEmLoop()].join(",");

// ── não-regressões ─────────────────────────────────────────────────────────
function* ifFalso() { if (false) { yield 9; } yield 1; }
const ramoNaoTomado = [...ifFalso()].join(",");

function* ifSemBloco() { if (true) yield 1; else yield 2; yield 3; }
const semChaves = [...ifSemBloco()].join(",");

function* ifComReturn() { if (true) { return; } yield 1; }
const comReturn = [...ifComReturn()].join(",");

function* semIf() { const a = yield 1; yield a * 3; }
const sd = semIf();
const semIfPrimeiro = sd.next().value;
const semIfEnviado = sd.next(4).value;

describe("if contendo yield na state-machine", () => {
  test("if com bloco: primeiro valor", () => expect(blocoPrimeiro).toBe(1));
  test("if com bloco: valor ENVIADO chega", () => expect(blocoEnviado).toBe(5));
  test("ramo then tomado", () => expect(thenEnviado).toBe(10));
  test("ramo else tomado", () => expect(peloElse).toBe(-1));
  test("if aninhado com dois yields de valor", () =>
    expect(an1 + "," + an2 + "," + an3).toBe("1,20,8"));
  test("if dentro de while", () => expect(noLoop).toBe("0,2"));
});

describe("não-regressões", () => {
  test("ramo não tomado não rende", () => expect(ramoNaoTomado).toBe("1"));
  test("if sem chaves", () => expect(semChaves).toBe("1,3"));
  test("if com return encerra", () => expect(comReturn).toBe(""));
  test("generator sem if: primeiro", () => expect(semIfPrimeiro).toBe(1));
  test("generator sem if: enviado", () => expect(semIfEnviado).toBe(12));
});
