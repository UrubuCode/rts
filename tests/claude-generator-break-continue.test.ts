import { describe, test, expect } from "rts:test";

// `break` num loop de generator TRAVAVA PARA SEMPRE — sem erro, sem saída:
//
//   function* a(){ let i=0; while(true){ if(i>=2) break; yield i; i=i+1; } }
//   [...a()]        // travava (exit 124 por timeout)  ·  Node: [0,1]
//
// `break`/`continue` não são modelados pela state-machine: o corpo do loop vira
// ESTADOS, e um `break` emitido verbatim dentro de um estado não tem loop de
// onde sair. A máquina ignorava o corte e o generator rodava indefinidamente.
//
// Correção: corpo com `break`/`continue` é INELEGÍVEL para a SM e cai no
// eager-buffer, que mantém o corpo verbatim e portanto respeita o corte. Perde-se
// a lazy nesses corpos; ganha-se terminar.
//
// Segunda correção, junto: o eager-buffer reescrevia `const r = yield v` para
// `push(v); const r = undefined` — o valor ENVIADO por `next(v)` não existe no
// modelo eager. Era um VALOR ERRADO em silêncio. Agora o `yield` fica intacto e
// o lowering recusa honestamente. Um corpo que a SM aceita não passa por aí (o
// value-yield já o torna elegível a lazy).
//
// Valores conferidos contra o Node. Pré-computado no top-level.

function* breakWhile() { let i = 0; while (true) { if (i >= 2) break; yield i; i = i + 1; } }
const doWhile = [...breakWhile()].join(",");

function* breakFor() { for (let i = 0; i < 5; i++) { if (i === 2) break; yield i; } }
const doFor = [...breakFor()].join(",");

function* continueForOf() { for (const x of [1, 2, 3]) { if (x === 2) continue; yield x; } }
const doContinue = [...continueForOf()].join(",");

function* breakDoWhile() { let i = 0; do { if (i >= 2) break; yield i; i = i + 1; } while (true); }
const doDoWhile = [...breakDoWhile()].join(",");

// ── não-regressões: o que DEVE continuar lazy ──────────────────────────────
function* infinito() { let i = 0; while (true) { yield i; i = i + 1; } }
const ii = infinito();
const infinitoA = ii.next().value;
const infinitoB = ii.next().value;

function* comSent() { let i = 0; while (i < 3) { const v = yield i; i = i + (v || 1); } }
const is = comSent();
const sentPrimeiro = is.next().value;
const sentEnviado = is.next(2).value;

function* semBreak() { let i = 0; while (i < 2) { yield i; i = i + 1; } }
const semCorte = [...semBreak()].join(",");

describe("break/continue em generator", () => {
  test("break em while(true) TERMINA", () => expect(doWhile).toBe("0,1"));
  test("break em for", () => expect(doFor).toBe("0,1"));
  test("continue em for-of", () => expect(doContinue).toBe("1,3"));
  test("break em do-while", () => expect(doDoWhile).toBe("0,1"));
});

describe("não-regressões: o caminho lazy é preservado", () => {
  test("generator INFINITO continua lazy", () =>
    expect(infinitoA + "," + infinitoB).toBe("0,1"));
  test("valor enviado por next(v) continua chegando", () =>
    expect(sentPrimeiro + "," + sentEnviado).toBe("0,2"));
  test("loop sem break não regrediu", () => expect(semCorte).toBe("0,1"));
});
