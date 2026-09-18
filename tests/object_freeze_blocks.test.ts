import { describe, test, expect } from "rts:test";

// Object.freeze bloqueia mutacoes; Object.seal permite atualizar uma chave
// existente mas nao acrescentar outra.
//
// Este ficheiro tem `import`, logo e um MODULO e o codigo de modulo e sempre
// strict. Em strict, escrever numa propriedade read-only ou acrescentar uma a
// um objeto nao extensivel LANCA TypeError; so em codigo sloppy e que a escrita
// falha em silencio. A versao anterior deste teste afirmava o caso sloppy
// ("ignorado") e morria com a excecao nao apanhada — Node 20 e Bun 1.4 lancam
// ambos nas tres escritas abaixo marcadas `lanca`, com o mesmo ficheiro.

function throwsTypeError(write: () => void): boolean {
  try {
    write();
    return false;
  } catch (e) {
    return e instanceof TypeError;
  }
}

const f = Object.freeze({ n: 42 } as { n: number; m?: number });
const w1 = throwsTypeError(() => { f.n = 99; });   // lanca: read-only
const w2 = throwsTypeError(() => { f.m = 1; });    // lanca: nao extensivel
const r1 = f.n;                                    // 42
const r1m = f.m;                                   // undefined

const s = Object.seal({ x: 1 } as { x: number; y?: number });
const w3 = throwsTypeError(() => { s.x = 100; });  // permitido
const w4 = throwsTypeError(() => { s.y = 200; });  // lanca: nao extensivel
const r2 = s.x;                                    // 100
const r3 = s.y;                                    // undefined

describe("object_freeze_blocks", () => {
  test("freeze lanca ao escrever em key existente e nao a altera", () => {
    expect(w1).toBe(true);
    expect(r1).toBe(42);
  });
  test("freeze lanca ao acrescentar key", () => {
    expect(w2).toBe(true);
    expect(r1m).toBe(undefined);
  });
  test("seal permite update", () => {
    expect(w3).toBe(false);
    expect(r2).toBe(100);
  });
  test("seal lanca ao acrescentar key", () => {
    expect(w4).toBe(true);
    expect(r3).toBe(undefined);
  });
});
