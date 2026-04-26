import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generic constraint <T extends X>: type-erased em runtime.

function add<T extends i64>(a: T, b: T): T {
  return a + b;
}

function max<T extends i64>(a: T, b: T): T {
  if (a > b) return a;
  return b;
}

const r1 = add<i64>(7, 8);
const h1 = gc.string_from_i64(r1);
print(h1); gc.string_free(h1);

const r2 = max<i64>(15, 23);
const h2 = gc.string_from_i64(r2);
print(h2); gc.string_free(h2);

describe("fixture:generic_constraint", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("15\n23\n");
  });
});
