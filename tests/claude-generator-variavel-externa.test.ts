import { describe, test, expect } from "rts:test";

// Um generator que passa pela STATE-MACHINE e escreve numa variável de FORA
// escrevia no próprio frame — o efeito colateral sumia em silêncio.
//
//   let c = 0;
//   function* g(){ while (true) { c = c + 1; yield c; if (c >= 2) return; } }
//   [...g()]     // [1,2] certinho nos dois
//   c            // Node: 2   ·   RTS: 0
//
// O generator PARECIA funcionar: os valores yieldados eram os certos, e só a
// escrita se perdia. Pré-existente (mesmo resultado no binário release anterior
// a esta frente).
//
// Causa: a máquina internava como slot de frame QUALQUER alvo de atribuição,
// incluindo um nome que o generator não declara. O prólogo da state-fn emite
// `let <nome> = FGET(…)` para cada local internado — o que SOMBREIA a variável
// de fora — e a escrita ia parar no frame.
//
// Como cheguei aqui, porque a rota importa: eu tinha reportado "o `finally` não
// roda". Estava errado. Instrumentando (um `console.log` dentro do `finally`)
// vi que ele RODA e imprime o valor novo; o que não acontecia era a escrita
// chegar na variável de fora. O sintoma que eu tinha nomeado era um caso
// particular deste bug, e o bug atinge qualquer escrita, dentro ou fora de
// `finally`.
//
// Correção: só vira slot de frame um nome que o generator DECLARA (parâmetro,
// `var`/`let`/`const`, binding de `for…of`/`in`, parâmetro de `catch`). Um nome
// livre continua sendo escrito onde vive.
//
// Todos os valores conferidos contra o Node.

// ── escrita numa global, generator com laço (state-machine) ───────────────
let contadorLoop = 0;
function* comLoop() {
  while (true) { contadorLoop = contadorLoop + 1; yield contadorLoop; if (contadorLoop >= 2) return; }
}
const loopValores = JSON.stringify([...comLoop()]);
const loopExterna = contadorLoop;

// ── escrita numa global, generator com yield de VALOR ─────────────────────
let acumulado = 0;
function* comValueYield() { const v = yield "ask"; acumulado = acumulado + 5; return v; }
const cv: any = comValueYield();
const cvPrimeiro = JSON.stringify(cv.next());
const cvSegundo = JSON.stringify(cv.next("V"));
const cvExterna = acumulado;

// ── escrita dentro de um `finally` (o caso que eu tinha diagnosticado mal) ─
let finRodou = 0;
function* comFinally() { try { yield 1; yield 2; } finally { finRodou = finRodou + 1; } }
const finValores = JSON.stringify([...comFinally()]);
const finExterna = finRodou;

// `finally` também roda quando o corpo termina por `return`
let finRodou2 = 0;
function* finallyComReturn() { try { yield 1; } finally { finRodou2 = finRodou2 + 1; } return "R"; }
const fr: any = finallyComReturn();
const frPrimeiro = JSON.stringify(fr.next());
const frSegundo = JSON.stringify(fr.next());
const frExterna = finRodou2;

// ── um LOCAL de mesmo papel continua no frame (não regrediu) ──────────────
function* comLocal() { let n = 0; while (n < 3) { n = n + 1; yield n; } return "n=" + n; }
const cl: any = comLocal();
const localSeq = JSON.stringify([cl.next(), cl.next(), cl.next(), cl.next()]);

// ── parâmetro continua no frame, e sobrevive à suspensão ─────────────────
function* comParam(p: any) { const a = yield p; p = p + "!"; return p + a; }
const cp: any = comParam("P");
const paramSeq = JSON.stringify([cp.next(), cp.next("A")]);

describe("generator: escrita em variavel externa", () => {
  test("laco: valores yieldados", () => expect(loopValores).toBe("[1,2]"));
  test("laco: a global de fora recebeu a escrita", () => expect(loopExterna).toBe(2));
  test("value-yield: primeiro passo", () =>
    expect(cvPrimeiro).toBe('{"value":"ask","done":false}'));
  test("value-yield: segundo passo", () =>
    expect(cvSegundo).toBe('{"value":"V","done":true}'));
  test("value-yield: a global de fora recebeu a escrita", () => expect(cvExterna).toBe(5));
  test("finally: valores yieldados", () => expect(finValores).toBe("[1,2]"));
  test("finally: rodou e a escrita chegou fora", () => expect(finExterna).toBe(1));
  test("finally com return: primeiro passo", () =>
    expect(frPrimeiro).toBe('{"value":1,"done":false}'));
  test("finally com return: segundo passo", () =>
    expect(frSegundo).toBe('{"value":"R","done":true}'));
  test("finally com return: rodou uma vez", () => expect(frExterna).toBe(1));
  test("local de mesmo papel continua no frame", () =>
    expect(localSeq).toBe(
      '[{"value":1,"done":false},{"value":2,"done":false},{"value":3,"done":false},{"value":"n=3","done":true}]',
    ));
  test("parametro sobrevive a suspensao", () =>
    expect(paramSeq).toBe('[{"value":"P","done":false},{"value":"P!A","done":true}]'));
});
