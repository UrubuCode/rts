import { describe, test, expect } from "rts:test";

// (cross-runtime #345) O 1o param de um tagged template tem tipo
// TemplateStringsArray. Antes do fix, from_annotation nao reconhecia esse
// tipo e caia no fallback ValTy::I64 -> `strings.length` retornava 0 e
// `strings.reduce(...)`/`for-of` crashavam (SIGILL), pois o array de strings
// nao era tratado como handle GC / array receiver. Agora vira ValTy::Handle.
//
// NOTA: o CONTEUDO do array de strings (valores por indice) e o rest param
// `...values` ainda tem bugs separados em alguns templates (follow-up); aqui
// testamos apenas o comprimento, que e' confiavel.

function partsCount(strings: TemplateStringsArray): number {
  return strings.length;
}

const a = partsCount`hello ${1} world ${2}!`; // 3 partes: "hello ", " world ", "!"
const b = partsCount`single`;                  // 1 parte
const c = partsCount`x${1}y`;                   // 2 partes

describe("tagged template TemplateStringsArray length (#345)", () => {
  test("3 parts", () => expect(`${a}`).toBe("3"));
  test("1 part", () => expect(`${b}`).toBe("1"));
  test("2 parts", () => expect(`${c}`).toBe("2"));
});
