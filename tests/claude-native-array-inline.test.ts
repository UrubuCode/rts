import { describe, test, expect } from "rts:test";

// Storage nativo de array (stack-slot inline) — corretude.
// Sob RTS_ARRAY_INLINE=1, arrays locais de tamanho fixo, não-escapantes, viram
// stack slot (load/store direto, sem call extern). Este teste valida que o
// resultado é IDÊNTICO ao caminho atual (HandleTable), com ou sem a flag —
// corretude não depende da flag. (O ganho de perf é medido em micro-bench, não
// aqui.)
//
// Pré-computado no top-level (ver nota CLAUDE.md sobre GC em test closures).

// Array local fixo via new Array(N) — qualifica para inline quando flag ON.
function sumIndexed(): number {
  const a: number[] = new Array(8);
  for (let i = 0; i < 8; i++) a[i] = i * 10;
  let s = 0;
  for (let i = 0; i < 8; i++) s = s + a[i];
  return s; // 0+10+...+70 = 280
}

// RMW inline: arr[i] += x e arr[i] = arr[i] + x.
function rmwIndexed(): number {
  const a: number[] = new Array(4);
  for (let i = 0; i < 4; i++) a[i] = 0;
  for (let k = 0; k < 1000; k++) {
    a[k % 4] = a[k % 4] + 1;
    a[(k + 1) % 4] += 2;
  }
  let s = 0;
  for (let i = 0; i < 4; i++) s = s + a[i];
  return s; // 1000*1 + 1000*2 = 3000
}

// Array literal fixo.
function literalArray(): number {
  const a: number[] = [5, 10, 15, 20];
  return a[0] + a[1] + a[2] + a[3]; // 50
}

// OOB read → 0 (JS: undefined, coage a 0 em soma). Bounds-check correto.
function oobRead(): number {
  const a: number[] = new Array(3);
  a[0] = 7;
  a[1] = 8;
  a[2] = 9;
  return a[0] + a[5] + a[2]; // 7 + 0 + 9 = 16 (a[5] OOB)
}

const r1 = sumIndexed();
const r2 = rmwIndexed();
const r3 = literalArray();
const r4 = oobRead();

describe("native array inline (corretude, flag-independente)", () => {
  test("array indexado fixo soma corretamente", () => {
    expect(r1).toBe(280);
  });
  test("RMW inline (+= e = self +) em array local", () => {
    expect(r2).toBe(3000);
  });
  test("array literal fixo", () => {
    expect(r3).toBe(50);
  });
  test("leitura fora de bounds devolve 0 (bounds-check)", () => {
    expect(r4).toBe(16);
  });
});
