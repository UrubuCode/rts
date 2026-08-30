import { test, expect } from "rts:test";

// O mesmo no do documento devolve sempre o MESMO objeto, e o que se poe nele
// fica la — inclusive quando chega como `event.target`.
//
// FIXTURE VERMELHA: pina um defeito ABERTO.
//
// Nao e purismo. E o que uma biblioteca que ANOTA nos assume, e sao todas: o
// React guarda o fiber em `no.__reactFiber$xyz` e vai busca-lo por
// `event.target.__reactFiber$xyz`; o jQuery guarda o cache de dados assim; o
// D3 guarda `__data__`. Com wrappers efemeros, escreve-se num objeto e le-se
// noutro.
//
// Medido no React 18 a correr neste motor: a aplicacao MONTA e PINTA — hooks,
// listas, estilos — e nenhum `onClick` dispara. E ja se sabe que nao e nada
// disto:
//
//   - o registo funciona: o React regista 132 listeners, `click` em captura e
//     em bubble;
//   - o despacho funciona: um clique no botao chega ao container do React nas
//     DUAS fases;
//   - o objeto de evento tem `type`, `target`, `currentTarget`,
//     `preventDefault`, `stopPropagation`, `eventPhase`, `bubbles`,
//     `defaultPrevented`, `nativeEvent` e `timeStamp`.
//
// O React recebe o evento e nao tem como o atribuir a um componente, porque o
// `target` que lhe chega e um wrapper diferente daquele onde ele escreveu.
//
// A correcao e uma cache de wrappers por `NodeId` do lado `.ts`, com o cuidado
// de nao segurar nos ja removidos do documento.

const doc = parseDocument("<button id='b'>x</button>");
const primeiro = doc.getElementById("b");
const segundo = doc.getElementById("b");

test("dois acessos ao mesmo no dao o mesmo objeto", function () {
  expect(primeiro === segundo).toBe(true);
});

test("uma propriedade posta num no sobrevive a outro acesso", function () {
  if (primeiro !== null) { (primeiro as any).__marca = 42; }
  const terceiro = doc.getElementById("b");
  expect(terceiro === null ? 0 : (terceiro as any).__marca).toBe(42);
});
