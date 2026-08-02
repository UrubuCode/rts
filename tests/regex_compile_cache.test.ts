import { describe, test, expect } from "rts:test";

// O automato de um regex é cacheado por (source, flags) para que um LITERAL
// dentro de um loop não reconstrua o NFA a cada iteração (medido: 1662 -> 40 ms
// em 20k iterações). O OBJETO continua novo a cada avaliação, que é o que a spec
// exige — estes testes fixam exatamente essa fronteira.

const freshPerEval: boolean[] = [];
for (let i = 0; i < 3; i++) freshPerEval.push(/a/g.test("aaa"));

const sameObj = /a/g;
const s1 = sameObj.test("aaa");
const li1 = sameObj.lastIndex;
const s2 = sameObj.test("aaa");
const li2 = sameObj.lastIndex;

const a = /a/g;
const b = /a/g;
a.test("aaa");
const aIdx = a.lastIndex;
const bIdx = b.lastIndex;

const meta = /ab+c/gi;
const fancyHit = /a(?=b)/.test("ab");
const fancyMiss = /a(?=b)/.test("ac");

const ctor = new RegExp("x+", "g");
const ctorHit = ctor.test("xxx");
const ctorIdx = ctor.lastIndex;

// Mesmo source, flags DIFERENTES: chaves de cache distintas.
const noGlobal = /a/;
noGlobal.test("aaa");

describe("regex automaton cache", () => {
  test("um literal /g/ avaliado em loop começa com lastIndex zerado", () => {
    expect(freshPerEval.length).toBe(3);
    expect(freshPerEval[0]).toBe(true);
    expect(freshPerEval[1]).toBe(true);
    expect(freshPerEval[2]).toBe(true);
  });

  test("o MESMO objeto /g/ avança lastIndex", () => {
    expect(s1).toBe(true);
    expect(li1).toBe(1);
    expect(s2).toBe(true);
    expect(li2).toBe(2);
  });

  test("dois objetos do mesmo source não compartilham lastIndex", () => {
    expect(aIdx).toBe(1);
    expect(bIdx).toBe(0);
  });

  test("source/flags sobrevivem ao cache", () => {
    expect(meta.source).toBe("ab+c");
    expect(meta.flags).toBe("gi");
    expect(meta.global).toBe(true);
  });

  test("o fallback fancy (lookahead) também é cacheado corretamente", () => {
    expect(fancyHit).toBe(true);
    expect(fancyMiss).toBe(false);
  });

  test("new RegExp com o mesmo source é um objeto próprio", () => {
    expect(ctorHit).toBe(true);
    expect(ctorIdx).toBe(3);
  });

  test("mesmo source com flags diferentes não colide no cache", () => {
    expect(noGlobal.global).toBe(false);
    expect(noGlobal.lastIndex).toBe(0);
  });
});
