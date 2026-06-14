import { describe, test, expect } from "rts:test";

// Inline de user-fn no call-site (RTS_INLINE_AST). Corretude flag-independente:
// o resultado deve ser idêntico com inline ON (default) e OFF. Cobre os casos
// de control-flow que o inline precisa redirecionar corretamente (return →
// store+jump ao join block).
//
// Pré-computado no top-level (ver nota CLAUDE.md sobre GC em test closures).

// (a) single-return — o caso elegível básico.
function pure(x: number): number { return (x * 16807) % 2147483647; }

// (b) if/else ambos retornam — os dois ramos fazem def_var+jump(join).
function maxOf(a: number, b: number): number {
  if (a > b) return a;
  else return b;
}

// (c) return em if sem else (fall-through).
function clamp(x: number): number {
  if (x < 0) return 0;
  return x;
}

// (e) inline aninhado: outer chama inner (depth).
function inner(x: number): number { return x + 1; }
function outer(x: number): number { return inner(x) * 2; }

// (f) assign-prelude a GLOBAL mutável + return (padrão LCG quente). O inline
// precisa copiar o store de `seed` na ordem certa antes do return. Corretude
// flag-independente: a sequência de `rnd()` deve ser idêntica ON/OFF.
let seed = 123456789;
function rnd(): number { seed = (seed * 16807) % 2147483647; return seed % 1000000; }

// (g) assign-prelude a LOCAL + compound assign (`+=`) + return.
function step(n: number): number { let acc = n; acc += n * 2; acc = acc % 7; return acc; }

// chamadas (callee em múltiplos call-sites também).
const r_pure1 = pure(123456789);
const r_pure2 = pure(987654321);
const r_max1 = maxOf(70, 1);
const r_max2 = maxOf(4, 9);
const r_clamp1 = clamp(-5);
const r_clamp2 = clamp(100);
const r_outer = outer(13); // (13+1)*2 = 28

// rnd() três vezes em sequência — cada call muta o global `seed`.
const r_rnd1 = rnd();
const r_rnd2 = rnd();
const r_rnd3 = rnd();
const r_seed_final = seed;
const r_step1 = step(5);  // acc=5; acc+=10 ->15; 15%7 ->1
const r_step2 = step(9);  // acc=9; acc+=18 ->27; 27%7 ->6

describe("user-fn inline (corretude, flag-independente)", () => {
  test("single-return inlinado", () => {
    expect(r_pure1).toBe((123456789 * 16807) % 2147483647);
    expect(r_pure2).toBe((987654321 * 16807) % 2147483647);
  });
  test("if/else ambos retornam", () => {
    expect(r_max1).toBe(70);
    expect(r_max2).toBe(9);
  });
  test("if sem else (fall-through)", () => {
    expect(r_clamp1).toBe(0);
    expect(r_clamp2).toBe(100);
  });
  test("inline aninhado outer→inner", () => {
    expect(r_outer).toBe(28);
  });
  test("assign-prelude a global mutável (LCG)", () => {
    // Valores deterministicos do LCG a partir de seed=123456789.
    // s1 = (123456789 * 16807) % 2147483647 = 1545653132 -> %1e6 = 653132
    const s1 = (123456789 * 16807) % 2147483647;
    const s2 = (s1 * 16807) % 2147483647;
    const s3 = (s2 * 16807) % 2147483647;
    expect(r_rnd1).toBe(s1 % 1000000);
    expect(r_rnd2).toBe(s2 % 1000000);
    expect(r_rnd3).toBe(s3 % 1000000);
    expect(r_seed_final).toBe(s3);
  });
  test("assign-prelude a local + compound (+=)", () => {
    expect(r_step1).toBe(1);
    expect(r_step2).toBe(6);
  });
});
