import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `expr satisfies T` — passthrough no codegen (igual `as`).
// Útil pra TS validar tipo sem alterar o tipo inferido do expr.

const x = 42 satisfies number;
const h = gc.string_from_i64(x);
print(h); gc.string_free(h); // 42

function compute(): number {
    return (10 + 5) satisfies number;
}

const h2 = gc.string_from_i64(compute());
print(h2); gc.string_free(h2); // 15

describe("fixture:satisfies_op", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n15\n");
  });
});
