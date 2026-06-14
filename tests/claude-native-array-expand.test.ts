import { describe, test, expect } from "rts:test";

// Expansão do storage nativo de array: tamanho via const + arrays top-level.
// Corretude flag-independente (ON==OFF). O ganho de perf é medido em bench.

// const-size: new Array(CONST) onde CONST é const-int → qualifica.
const N = 8;
function constSized(): number {
  const a: number[] = new Array(N);
  for (let i = 0; i < N; i++) a[i] = i + 1;
  let s = 0;
  for (let i = 0; i < N; i++) s = s + a[i];
  return s; // 1+2+...+8 = 36
}

// const-size com RMW.
function constRmw(): number {
  const SIZE = 4;
  const a: number[] = new Array(SIZE);
  for (let i = 0; i < SIZE; i++) a[i] = 0;
  for (let k = 0; k < 100; k++) a[k % SIZE] += 1;
  return a[0] + a[1] + a[2] + a[3]; // 100
}

const r1 = constSized();
const r2 = constRmw();

describe("native array expand (const-size + top-level)", () => {
  test("new Array(CONST) qualifica", () => {
    expect(r1).toBe(36);
  });
  test("RMW em array const-dimensionado", () => {
    expect(r2).toBe(100);
  });
});
