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

// (cross-runtime #345 followup) Conteudo do array por indice + rest values.
// O empacotamento de rest do tagged template empacotava a cookedArray no
// proprio `strings` quando a tag tinha 1 param (TemplateStringsArray virou
// Handle e disparava a heuristica de rest), e nao normalizava o array de
// rest para 0/1 valores. Corrigido: slot 0 eh sempre `strings`, rest sempre
// recebe array (mesmo vazio).
function firstTwo(strings: TemplateStringsArray): string {
  return strings[0] + "|" + strings[1];
}
function restCount(strings: TemplateStringsArray, ...values: any[]): number {
  return values.length;
}

const a = partsCount`hello ${1} world ${2}!`; // 3 partes
const b = partsCount`single`;                  // 1 parte
const c = partsCount`x${1}y`;                   // 2 partes
const d = firstTwo`AAA${1}BBB`;                 // "AAA|BBB"
const e = restCount`a${1}b${2}c`;               // 2
const f = restCount`a${1}b`;                    // 1
const g = restCount`ab`;                        // 0

describe("tagged template TemplateStringsArray (#345)", () => {
  test("3 parts", () => expect(`${a}`).toBe("3"));
  test("1 part", () => expect(`${b}`).toBe("1"));
  test("2 parts", () => expect(`${c}`).toBe("2"));
  test("strings[0]|strings[1]", () => expect(d).toBe("AAA|BBB"));
  test("rest values length 2", () => expect(`${e}`).toBe("2"));
  test("rest values length 1", () => expect(`${f}`).toBe("1"));
  test("rest values length 0", () => expect(`${g}`).toBe("0"));
});
