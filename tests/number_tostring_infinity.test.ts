import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `(1/0).toString()` retornava "inf" (formato
// Rust) em vez de "Infinity" (JS). NUMBER_TO_STRING_RADIX caia em
// format!("{v}") sem tratar NaN/Infinity. Fix: guard no inicio (igual toFixed).

let out = "";
function print(v: string): void { out += v + "\n"; }

print((1 / 0).toString());        // Infinity
print((-1 / 0).toString());       // -Infinity
print((0 / 0).toString());        // NaN
const inf = Infinity;
print(inf.toString());            // Infinity
print((Number("x")).toString());  // NaN
// radix tambem
print((255).toString(16));        // ff (caso normal preservado)
print((10).toString());           // 10
print((3.14).toString());         // 3.14

describe("number toString infinity", () => {
  test("Infinity/NaN formatados como JS", () =>
    expect(out).toBe("Infinity\n-Infinity\nNaN\nInfinity\nNaN\nff\n10\n3.14\n"));
});
