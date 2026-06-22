import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `as const`, non-null `!`, satisfies (todos no-op no codegen).

function maybe(): number {
    return 7;
}

const v = maybe()!;          // non-null: passthrough
const c = (3 + 4) as const;  // as const: passthrough

print(`${v}`); // 7
print(`${c}`); // 7

describe("fixture:type_assertion_misc", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("7\n7\n");
  });
});
