import { describe, test, expect } from "rts:test";

// `split` tem dois caminhos desde 2026-08-13: um estreito, que fatia os bytes
// do sujeito sem construir uma `String` por pedaço, e o geral em `pattern.rs`.
// O que este ficheiro fixa é que os dois respondem a MESMA coisa — cada caso
// aqui foi conferido contra o Node.
//
// As quatro regras que o caminho estreito recusa de propósito, em vez de
// reimplementar, são as quatro primeiras: separador ausente, separador vazio,
// expressão regular e sujeito largo. Cada uma tem uma regra própria na
// especificação e reescrevê-la ali seria a segunda cópia que faz as duas
// grafias discordarem.

describe("split responde o mesmo pelos dois caminhos", () => {
  test("o caso comum: texto estreito, separador literal", () => {
    expect(JSON.stringify("a,b,c".split(","))).toBe(JSON.stringify(["a", "b", "c"]));
    expect(JSON.stringify("a--b--c".split("--"))).toBe(JSON.stringify(["a", "b", "c"]));
  });

  test("uma peça vazia na ponta é uma peça", () => {
    // Um laço que só empurrasse a cada acerto responderia uma peça a menos.
    expect(JSON.stringify("a,".split(","))).toBe(JSON.stringify(["a", ""]));
    expect(JSON.stringify(",a".split(","))).toBe(JSON.stringify(["", "a"]));
    expect(JSON.stringify("".split(","))).toBe(JSON.stringify([""]));
    expect(JSON.stringify(",".split(","))).toBe(JSON.stringify(["", ""]));
  });

  test("acertos que se sobrepõem contam uma vez, da esquerda", () => {
    expect(JSON.stringify("aaa".split("aa"))).toBe(JSON.stringify(["", "a"]));
  });

  test("separador ausente é a string inteira, não cada caractere", () => {
    expect(JSON.stringify("abc".split())).toBe(JSON.stringify(["abc"]));
  });

  test("separador vazio divide entre cada unidade e não deixa cauda", () => {
    expect(JSON.stringify("abc".split(""))).toBe(JSON.stringify(["a", "b", "c"]));
  });

  test("uma expressão regular continua a passar pelo caminho geral", () => {
    expect(JSON.stringify("a1b22c".split(/[0-9]+/))).toBe(JSON.stringify(["a", "b", "c"]));
  });

  test("texto largo responde igual ao estreito", () => {
    // `é` e `ö` cabem em Latin-1 e são estreitos; um emoji não é, e é o que
    // manda o sujeito para o caminho geral.
    expect(JSON.stringify("héllo,wörld".split(","))).toBe(JSON.stringify(["héllo", "wörld"]));
    expect(JSON.stringify("a🙂b,c".split(","))).toBe(JSON.stringify(["a🙂b", "c"]));
  });

  test("o limite corta as peças e um limite negativo não corta nada", () => {
    expect(JSON.stringify("a,b,c".split(",", 2))).toBe(JSON.stringify(["a", "b"]));
    expect(JSON.stringify("a,b,c".split(",", 0))).toBe(JSON.stringify([]));
    expect(JSON.stringify("a,b".split(",", -1))).toBe(JSON.stringify(["a", "b"]));
  });

  test("as peças são strings de verdade, com os métodos delas", () => {
    // O caminho estreito constrói cada peça a partir dos bytes, então uma peça
    // que não fosse interna como célula responderia `undefined` aqui.
    const pieces = "um,dois,tres".split(",");
    expect(pieces.length).toBe(3);
    expect(pieces[1].length).toBe(4);
    expect(pieces[1].toUpperCase()).toBe("DOIS");
    expect(pieces[2] === "tres").toBe(true);
  });
});
