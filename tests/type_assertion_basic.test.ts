import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Type assertion `as` em expressão simples.

function getValue(): number {
    return 42;
}

const x = getValue() as number;
const h = gc.string_from_i64(x);
print(h); gc.string_free(h); // 42

// Forma legacy <Type>expr também aceita.
const y = (10 as number) + 5;
const h2 = gc.string_from_i64(y);
print(h2); gc.string_free(h2); // 15

describe("fixture:type_assertion_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n15\n");
  });
});
