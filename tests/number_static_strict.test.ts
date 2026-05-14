// #872 fix: Number.isFinite/isNaN/isInteger/isSafeInteger NAO coagem
// (retornam false para non-number); global isNaN/isFinite coagem string.
import { describe, test, expect } from "rts:test";

let out = "";
out += "n_isFinite_str=" + Number.isFinite("5") + "\n";       // false
out += "n_isNaN_str=" + Number.isNaN("NaN") + "\n";           // false
out += "n_isInt_str=" + Number.isInteger("5") + "\n";         // false
out += "n_isSafe_str=" + Number.isSafeInteger("5") + "\n";    // false
out += "n_isFinite_5=" + Number.isFinite(5) + "\n";           // true
out += "n_isNaN_NaN=" + Number.isNaN(NaN) + "\n";             // true
out += "n_isInt_5=" + Number.isInteger(5) + "\n";             // true
out += "n_isSafe_5=" + Number.isSafeInteger(5) + "\n";        // true

let g = "";
g += "g_isNaN_str=" + isNaN("NaN") + "\n";       // true (coage "NaN" → NaN)
g += "g_isNaN_5=" + isNaN("5") + "\n";           // false (coage "5" → 5)
g += "g_isFinite_str=" + isFinite("5") + "\n";   // true (coage)
g += "g_isFinite_bad=" + isFinite("abc") + "\n"; // false (NaN)

describe("Number.is* strict + global is* coerce (#872)", () => {
  test("Number.is* sem coercao", () => expect(out).toBe(
    "n_isFinite_str=false\nn_isNaN_str=false\nn_isInt_str=false\nn_isSafe_str=false\n" +
    "n_isFinite_5=true\nn_isNaN_NaN=true\nn_isInt_5=true\nn_isSafe_5=true\n"
  ));
  test("global is* com coercao", () => expect(g).toBe(
    "g_isNaN_str=true\ng_isNaN_5=false\ng_isFinite_str=true\ng_isFinite_bad=false\n"
  ));
});
