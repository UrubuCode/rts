import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Type assertion `as` em expressão simples.

function getValue(): number {
    return 42;
}

const x = getValue() as number;
print(`${x}`); // 42

// Forma legacy <Type>expr também aceita.
const y = (10 as number) + 5;
print(`${y}`); // 15

describe("fixture:type_assertion_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n15\n");
  });
});
