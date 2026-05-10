// Cobertura do spread universal — `[...iter]` agora aceita Vec/String/Map
// via runtime helper SPREAD_INTO_VEC. Antes so' Vec funcionava (chamava
// VEC_LEN/VEC_GET, retornava len=-1 em string -> array vazio).
import { describe, test, expect } from "rts:test";

// Array spread (caso original que ja funcionava)
const a1: number[] = [1, 2, 3];
const a2: number[] = [...a1, 4, 5];
const a3: number[] = [0, ...a1, 99];
const a4: number[] = [...a1];

// String spread (NOVO — antes retornava vazio)
const s1: string[] = [..."abc"] as any;
const s2: string[] = [..."hello"] as any;
const s3: string[] = [..."" as string] as any;

// Spread em call args
function sum3(a: number, b: number, c: number): number { return a + b + c; }
const args = [10, 20, 30];
const r = sum3(...args);

// Object spread (caso original)
const o1 = { x: 1, y: 2 };
const o2 = { ...o1, z: 3 };
const o3 = { ...o1, x: 99 };

describe("Spread universal — Vec/String em [...x] (#spread)", () => {
  test("array spread tail", () => expect(a2.length).toBe(5));
  test("array spread mid", () => expect(a3.length).toBe(5));
  test("array spread copy", () => expect(a4.length).toBe(3));
  test("string spread chars", () => expect(s1.length).toBe(3));
  test("string spread longer", () => expect(s2.length).toBe(5));
  test("string spread empty", () => expect(s3.length).toBe(0));
  test("call args spread", () => expect(r).toBe(60));
  test("object spread", () => expect(o2.z).toBe(3));
  test("object spread override", () => expect(o3.x).toBe(99));
});
