import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Tag fn recebe (strings_array, ...interpolated_values).
// Caso 1 — soma de valores interpolados, ignora strings.
function sum(strings: string[], a: number, b: number): number {
  return a + b;
}
print(`${sum`x${10}y${20}z`}`);
print(`${sum`${5}+${7}`}`);

// Caso 2 — multiplica todos os interpolados.
function mul3(strings: string[], a: number, b: number, c: number): number {
  return a * b * c;
}
print(`${mul3`r=${2},${3},${4}`}`);

// Caso 3 — tag pode receber 0 valores interpolados.
function noVals(strings: string[]): number {
  return 99;
}
print(`${noVals`only static text`}`);

describe("tagged_template", () => {
  test("call with interpolated values", () =>
    expect(__rtsCapturedOutput).toBe("30\n12\n24\n99\n"));
});
