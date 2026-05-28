import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `arr.map((c, i) => c + i)` onde arr eh array-de-
// strings (split, array literal de strings) dava lixo — o slot 0 (elem) era
// tratado como i64, entao `c + i` somava handle+idx em vez de concatenar.
// Fix: receiver_is_string_array marca slot0_is_string p/ map/forEach/filter/etc.

let out = "";
function print(v: string): void { out += v + "\n"; }

// split chain + callback 2-param
print("abc".split("").map((c, i) => c + i).join(","));   // a0,b1,c2

// array literal de strings
print(["x", "y", "z"].map((c, i) => c + i).join(","));   // x0,y1,z2

// callback 1-param continua OK
print("ab".split("").map(c => c + "!").join(","));       // a!,b!

// number array com idx NAO afetado (slot0 nao string)
print([10, 20].map((x, i) => x + i).join(","));          // 10,21

// filter sobre string array com idx
print(["a", "bb", "ccc"].filter((s, i) => s.length > i).join(",")); // a,bb,ccc

// split().slice().map preserva string
print("a,b,c,d".split(",").slice(1).map((c, i) => c + i).join(",")); // b0,c1,d2

describe("string array map com idx", () => {
  test("slot 0 string em map/filter de array-de-strings", () =>
    expect(out).toBe("a0,b1,c2\nx0,y1,z2\na!,b!\n10,21\na,bb,ccc\nb0,c1,d2\n"));
});
