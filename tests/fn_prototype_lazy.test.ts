import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR1) fn.prototype eh lazy-alocado como Map handle.
// Acessos repetidos retornam handle nao-zero.

function Animal(): void {}

const p1: number = Animal.prototype as any;

print("p1_nonzero=" + (p1 !== 0));

describe("fn.prototype lazy alloc (#264 PR1)", () => {
  test("retorna handle nao-zero", () =>
    expect(__rtsCapturedOutput).toBe(
      "p1_nonzero=true\n"
    ));
});
