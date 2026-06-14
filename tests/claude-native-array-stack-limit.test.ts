import { describe, test, expect } from "rts:test";

// Limite de tamanho do storage nativo (MAX_NATIVE_ARRAY_LEN=1024): arrays
// grandes ficam no heap (caminho atual) em vez de virar stack slot, senão
// vários/grandes arrays estouram a stack (SEGFAULT — visto em company_sim com
// 5 arrays de 10000). Corretude flag-independente; o teste garante que arrays
// grandes não crasham e produzem o resultado certo.

function bigArray(): number {
  const a: number[] = new Array(5000); // > 1024 → heap, não stack slot
  for (let i = 0; i < 5000; i++) a[i] = i;
  let s = 0;
  for (let i = 0; i < 5000; i++) s = s + a[i];
  return s; // 0+1+...+4999 = 12497500
}

function smallArray(): number {
  const a: number[] = new Array(64); // <= 1024 → stack slot (inline)
  for (let i = 0; i < 64; i++) a[i] = i;
  let s = 0;
  for (let i = 0; i < 64; i++) s = s + a[i];
  return s; // 0+...+63 = 2016
}

// Vários arrays grandes na mesma fn (o caso que segfaultava).
function manyBig(): number {
  const a: number[] = new Array(3000);
  const b: number[] = new Array(3000);
  const c: number[] = new Array(3000);
  for (let i = 0; i < 3000; i++) { a[i] = 1; b[i] = 2; c[i] = 3; }
  let s = 0;
  for (let i = 0; i < 3000; i++) s = s + a[i] + b[i] + c[i];
  return s; // 3000 * 6 = 18000
}

const r1 = bigArray();
const r2 = smallArray();
const r3 = manyBig();

describe("native array stack limit", () => {
  test("array grande (>1024) funciona (heap, sem segfault)", () => {
    expect(r1).toBe(12497500);
  });
  test("array pequeno (<=1024) funciona (stack slot)", () => {
    expect(r2).toBe(2016);
  });
  test("vários arrays grandes na mesma fn não estouram a stack", () => {
    expect(r3).toBe(18000);
  });
});
