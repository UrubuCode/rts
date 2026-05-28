import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): arr.at(oor) / pop()/shift() de vazio retornavam
// handle-string "undefined" em vez do sentinel — `?? -1` e `=== undefined`
// nao disparavam. Fix: at usa sentinel i64::MIN+2; VEC_POP/VEC_SHIFT idem.
// (Mesma familia do find/findLast #1257.)

let out = "";
function print(v: string): void { out += v + "\n"; }

const arr = [1, 2, 3];

// at fora de range -> undefined real
print((arr.at(10) === undefined) + "");   // true
print((arr.at(10) ?? -1) + "");           // -1
print(("" + arr.at(10)));                 // undefined
// at valido (inclui negativo)
print((arr.at(0) ?? -1) + "");            // 1
print((arr.at(-1) ?? -1) + "");           // 3

// pop/shift de vazio -> undefined real
const empty: number[] = [];
print((empty.pop() ?? -1) + "");          // -1
print((empty.shift() ?? -1) + "");        // -1

// pop/shift com valor continuam OK
const ne = [7, 8];
print((ne.pop() ?? -1) + "");             // 8
print((ne.shift() ?? -1) + "");           // 7

describe("at/pop/shift undefined", () => {
  test("sem valor -> undefined real (?? e === funcionam)", () =>
    expect(out).toBe("true\n-1\nundefined\n1\n3\n-1\n-1\n8\n7\n"));
});
