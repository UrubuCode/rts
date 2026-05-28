import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): find/findLast SEM match retornavam handle-string
// "undefined" em vez do sentinel undefined real. Consequencia: `?? -1` e
// `=== undefined` nao disparavam (typeof dava "string"). Fix: runtime retorna
// i64::MIN+2 (sentinel) no caso None — template/console ainda formata
// "undefined" via TPL_COERCE_AUTO.

let out = "";
function print(v: string): void { out += v + "\n"; }

// find sem match -> undefined real
const r1 = [1, 2, 3].find(x => x === 99);
print((r1 === undefined) + "");        // true
print((r1 ?? -1) + "");                // -1
print(("" + r1));                      // undefined

// find com match continua OK
const r2 = [1, 2, 3].find(x => x === 2);
print((r2 ?? -1) + "");                // 2

// findLast sem match com captura local + ??
function lastNone(arr: number[], lim: number): number {
  return arr.findLast(x => x > lim) ?? -1;
}
print(lastNone([1, 2, 3], 10) + "");   // -1

// findLast com match
function lastMatch(arr: number[], lim: number): number {
  return arr.findLast(x => x > lim) ?? -1;
}
print(lastMatch([1, 5, 2, 8], 3) + ""); // 8

describe("find undefined sentinel", () => {
  test("find/findLast sem match -> undefined real (?? e === funcionam)", () =>
    expect(out).toBe("true\n-1\nundefined\n2\n-1\n8\n"));
});
