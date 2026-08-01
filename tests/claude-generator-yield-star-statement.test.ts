import { describe, test, expect } from "rts:test";

// Regressão do `yield*` em posição de STATEMENT, medida ao fatorar o laço de
// delegação para servir também a posição de VALOR
// (`claude-generator-yield-star-valor.test.ts`).
//
// A forma statement NÃO ficou byte a byte idêntica, e a diferença é
// deliberada: `DELEGATE_NEXT` passou a receber o generator EXTERNO para
// encaminhar ao delegado o valor de `outer.next(v)`, como manda a spec. Isso
// vale para as duas posições, então o caminho statement mudou junto — vide o
// caso "next(v) durante delegação em statement" abaixo, que ANTES lia
// `undefined` no `const q = yield …` de dentro do delegado e agora lê o valor
// enviado, igual ao Node.
//
// Todos os valores conferidos contra o Node.
//
// GAP CONHECIDO, PRÉ-EXISTENTE E NÃO CORRIGIDO AQUI: `yield* new Set([1,2])` /
// `yield* new Map(...)` produzem `[]` em vez dos elementos. Medido no binário
// release ANTERIOR a esta frente: mesmo `[]`, ou seja, não é regressão.
// Causa: `DELEGATE_START` reconhece só GenState/Vec/String e trata qualquer
// outra fonte como esgotada de imediato. É a mesma limitação que motivou o
// for-of da state-machine a usar o protocolo `__rtsadp_iter_*` em vez desses
// helpers. O conserto é rotear a fonte desconhecida por `iter_open`; fica
// registrado como decisão do lead, não silenciado.

function* fromArray() { yield* [1, 2]; yield 3; }
const arrayDeleg = JSON.stringify([...fromArray()]);

function* fromString() { yield* "ab"; yield "c"; }
const stringDeleg = JSON.stringify([...fromString()]);

function* src() { yield "x"; yield "y"; }
function* fromGen() { yield* src(); yield "z"; }
const genDeleg = JSON.stringify([...fromGen()]);

function* twoDelegs() { yield* [1]; yield* [2, 3]; }
const doisSeguidos = JSON.stringify([...twoDelegs()]);

function* inLoop(rows: any) { for (const r of rows) { yield* r; } }
const dentroDeLaco = JSON.stringify([...inLoop([[1, 2], [3]])]);

function* leafS() { yield* [1, 2]; }
function* midS() { yield* leafS(); yield 3; }
const aninhado = JSON.stringify([...midS()]);

function* empty() { }
function* fromEmpty() { yield* empty(); yield "only"; }
const delegadoVazio = JSON.stringify([...fromEmpty()]);

// O caso que mudou: `next(v)` atravessa a delegação em posição de statement.
function* innerV() { const q = yield "ask"; yield "inner:" + q; }
function* outerV() { yield* innerV(); yield "after"; }
const ov: any = outerV();
const sentEmStatement = JSON.stringify([ov.next(), ov.next("V"), ov.next(), ov.next()]);

describe("generator: yield* em statement (regressao)", () => {
  test("delegar array", () => expect(arrayDeleg).toBe("[1,2,3]"));
  test("delegar string", () => expect(stringDeleg).toBe('["a","b","c"]'));
  test("delegar outro generator", () => expect(genDeleg).toBe('["x","y","z"]'));
  test("dois yield* seguidos", () => expect(doisSeguidos).toBe("[1,2,3]"));
  test("yield* dentro de laco", () => expect(dentroDeLaco).toBe("[1,2,3]"));
  test("delegacao aninhada", () => expect(aninhado).toBe("[1,2,3]"));
  test("delegado vazio", () => expect(delegadoVazio).toBe('["only"]'));
  test("next(v) durante delegacao em statement", () =>
    expect(sentEmStatement).toBe(
      '[{"value":"ask","done":false},{"value":"inner:V","done":false},{"value":"after","done":false},{"done":true}]',
    ));
});
