import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

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

const h1 = gc.string_from_i64(v);
print(h1); gc.string_free(h1); // 7
const h2 = gc.string_from_i64(c);
print(h2); gc.string_free(h2); // 7

describe("fixture:type_assertion_misc", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("7\n7\n");
  });
});
