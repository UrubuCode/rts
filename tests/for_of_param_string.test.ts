import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `for (const ch of s)` onde s eh PARAMETRO string
// nao iterava (vazio/crash). for_of_iterates_string so' reconhecia
// local_string_vars (var top-level/local), nao params anotados `: string`.
// Fix: param `: string` marca local_string_vars em user_fn.rs (mesma rota do
// local_map_vars #1261).

let out = "";
function print(v: string): void { out += v + "\n"; }

function countChar(s: string, c: string): number {
  let n = 0;
  for (const ch of s) {
    if (ch === c) n++;
  }
  return n;
}
print(countChar("banana", "a") + "");   // 3
print(countChar("hello", "l") + "");     // 2

function upper(s: string): string {
  let r = "";
  for (const ch of s) r += ch.toUpperCase();
  return r;
}
print(upper("abc"));                      // ABC

function reverse(s: string): string {
  let r = "";
  for (const ch of s) r = ch + r;
  return r;
}
print(reverse("hello"));                  // olleh

// var local string continua OK (guard)
const local = "xyz";
let lc = "";
for (const ch of local) lc += ch + ".";
print(lc);                                // x.y.z.

describe("for-of param string", () => {
  test("for-of sobre param string itera chars", () =>
    expect(out).toBe("3\n2\nABC\nolleh\nx.y.z.\n"));
});
