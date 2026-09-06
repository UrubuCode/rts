import { describe, test, expect } from "rts:test";

// `m.groups` tem de sobreviver ao COLETOR — e não sobrevivia.
//
// `regex::groups_object` pedia a célula do objeto a `native::plain` e guardava-a
// num `u32` cru de um local do Rust. A seguir, dentro do laço, chamava
// `intern_value` uma vez por grupo — e `intern_value` insere no slab e chama o
// alocador, portanto COLETA. A varredura da pilha é conservativa: reconhece
// palavras que sejam referências CODIFICADAS, e um índice cru não é nenhuma.
// Durante o laço, o objeto que o chamador ia receber era invisível.
//
// O que ele pina é a ESCALA, e por isso o número de iterações é o teste. Medido
// a 2026-09-06 contra um binário release da árvore anterior à correção:
// `m.groups.num` respondia certo para 1, 2, 10 e 1 000 execuções e passava a
// `undefined` a partir da 12 562 — ou seja, até à primeira coleta. Um teste que
// executasse o `exec` meia dúzia de vezes passa com o defeito presente, que é
// como ele sobreviveu: ninguém escreve um teste de grupos nomeados com um laço.
//
// Sob a pressão de alocação de `bench/analytic.ts` a resposta errada passava a
// violação de acesso, portanto isto não é só uma resposta errada — é um
// processo que morre.

const padrao = /[a-f]+(?<num>[0-9]+)/;

let primeiroErrado = -1;
let i = 0;
while (i < 60000) {
  const m = padrao.exec("abc123");
  const g = m === null ? undefined : (m.groups as any).num;
  if (g !== "123" && primeiroErrado < 0) primeiroErrado = i;
  i = i + 1;
}

// O mesmo pelo lado do `indices`, que constrói um segundo objeto de grupos.
const comIndices = /[a-f]+(?<num>[0-9]+)/d;
let indicesErrado = -1;
let k = 0;
while (k < 60000) {
  const m = comIndices.exec("abc123") as any;
  const par = m === null || m.indices === undefined ? undefined : m.indices.groups.num;
  if ((par === undefined || par[0] !== 3) && indicesErrado < 0) indicesErrado = k;
  k = k + 1;
}

describe("um grupo nomeado sobrevive à coleta de lixo", () => {
  test("m.groups continua a responder depois de milhares de execuções", () => {
    expect(primeiroErrado).toBe(-1);
  });

  test("m.indices.groups também", () => {
    expect(indicesErrado).toBe(-1);
  });
});
