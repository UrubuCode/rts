import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `expr satisfies T` — passthrough no codegen (igual `as`).
// Útil pra TS validar tipo sem alterar o tipo inferido do expr.

const x = 42 satisfies number;
print(`${x}`); // 42

function compute(): number {
    return (10 + 5) satisfies number;
}

print(`${compute()}`); // 15

describe("fixture:satisfies_op", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n15\n");
  });
});
