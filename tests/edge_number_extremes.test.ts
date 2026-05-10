// Edge cases criativos para Number — limites, NaN, divisao por zero,
// coerc\xF5es exoticas. Bate em codegen do `/`, comparacoes mistas e
// classes Number/JSON.
import { describe, test, expect } from "rts:test";

const MAX_SAFE = 9007199254740991;
const MIN_SAFE = -9007199254740991;
const positiveZeroDiv: number = 1 / 0;
const negativeZeroDiv: number = -1 / 0;
const nanFromZero: number = 0 / 0;
const isNanResult: boolean = Number.isNaN(nanFromZero);
const isFiniteInf: boolean = Number.isFinite(positiveZeroDiv);
const isFiniteNan: boolean = Number.isFinite(nanFromZero);
const isIntegerHalf: boolean = Number.isInteger(0.5);
const isIntegerInt: boolean = Number.isInteger(42);
const isIntegerBig: boolean = Number.isInteger(MAX_SAFE);
const safeIntegerOver: boolean = Number.isSafeInteger(MAX_SAFE + 2);
const safeIntegerExact: boolean = Number.isSafeInteger(MAX_SAFE);

// NaN nunca eh igual a si mesmo
const nanEqNan: boolean = nanFromZero === nanFromZero;
const nanNotEqNan: boolean = nanFromZero !== nanFromZero;

// Infinity em comparacoes
const infGtMax: boolean = positiveZeroDiv > MAX_SAFE;
const negInfLtMin: boolean = negativeZeroDiv < MIN_SAFE;
const infMinusInf: number = positiveZeroDiv - positiveZeroDiv;
const isInfMinusInfNan: boolean = Number.isNaN(infMinusInf);

describe("Number — extremos e NaN", () => {
  test("1/0 = Infinity", () => expect(positiveZeroDiv === Infinity).toBe(true));
  test("-1/0 = -Infinity", () => expect(negativeZeroDiv === -Infinity).toBe(true));
  test("0/0 = NaN (via isNaN)", () => expect(isNanResult).toBe(true));
  test("isFinite(Infinity) false", () => expect(isFiniteInf).toBe(false));
  test("isFinite(NaN) false", () => expect(isFiniteNan).toBe(false));
  test("isInteger(0.5) false", () => expect(isIntegerHalf).toBe(false));
  test("isInteger(42) true", () => expect(isIntegerInt).toBe(true));
  test("isInteger(MAX_SAFE) true", () => expect(isIntegerBig).toBe(true));
  test("isSafeInteger(MAX_SAFE+2) false", () => expect(safeIntegerOver).toBe(false));
  test("isSafeInteger(MAX_SAFE) true", () => expect(safeIntegerExact).toBe(true));
  test("NaN === NaN é false (JS spec)", () => expect(nanEqNan).toBe(false));
  test("NaN !== NaN é true (JS spec)", () => expect(nanNotEqNan).toBe(true));
  test("Infinity > MAX_SAFE", () => expect(infGtMax).toBe(true));
  test("-Infinity < MIN_SAFE", () => expect(negInfLtMin).toBe(true));
  test("Infinity - Infinity = NaN", () => expect(isInfMinusInfNan).toBe(true));
});
